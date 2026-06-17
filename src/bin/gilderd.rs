use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;
#[cfg(feature = "gtk-renderer")]
use std::{cell::RefCell, rc::Rc};

use gilder::config::{ApplicationPaths, GilderConfig, PerformanceConfig};
use gilder::ipc::RequestMethod;
use gilder::renderer::StaticRenderSyncPlan;
use gilder::state::AppState;
use serde_json::{Value, json};

fn main() {
    if let Err(err) = run() {
        eprintln!("gilderd: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let context = load_daemon_context()?;
    let listener = bind_ipc_listener()?;
    let renderer_updates = renderer_update_senders();

    #[cfg(feature = "gtk-renderer")]
    {
        run_gtk_daemon(context, listener, renderer_updates)
    }

    #[cfg(not(feature = "gtk-renderer"))]
    {
        run_ipc_daemon(context, listener, renderer_updates);
        Ok(())
    }
}

fn load_daemon_context() -> Result<DaemonContext, String> {
    let paths = ApplicationPaths::from_env().map_err(|err| err.to_string())?;
    let config = GilderConfig::load(&paths.config_file)
        .map_err(|err| format!("failed to load {}: {err}", paths.config_file.display()))?;
    let state = gilder::state::load_state(&paths.state_file)
        .map_err(|err| format!("failed to load {}: {err}", paths.state_file.display()))?;
    let desktop = gilder::desktop::adapters::read_desktop_snapshot(&config.adapters);
    Ok(DaemonContext {
        paths,
        config,
        state,
        desktop,
    })
}

fn bind_ipc_listener() -> Result<UnixListener, String> {
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

    Ok(listener)
}

fn renderer_update_senders() -> Vec<mpsc::Sender<StaticRenderSyncPlan>> {
    #[cfg(any(
        not(feature = "video-renderer"),
        all(feature = "video-renderer", feature = "gtk-renderer")
    ))]
    {
        Vec::new()
    }

    #[cfg(all(feature = "video-renderer", not(feature = "gtk-renderer")))]
    {
        let mut senders = Vec::new();

        let (sender, receiver) = mpsc::channel::<StaticRenderSyncPlan>();
        spawn_video_renderer_loop(receiver);
        senders.push(sender);

        senders
    }
}

#[cfg(not(feature = "gtk-renderer"))]
fn run_ipc_daemon(
    context: DaemonContext,
    listener: UnixListener,
    renderer_updates: Vec<mpsc::Sender<StaticRenderSyncPlan>>,
) {
    let runtime = Arc::new(DaemonRuntime::new(context, renderer_updates));
    match refreshed_render_sync(&runtime) {
        Ok(sync) => {
            runtime.queue_render_sync_if_changed(sync);
        }
        Err(err) => eprintln!("gilderd: failed to prepare initial render sync: {err}"),
    }
    spawn_desktop_refresh_loop(Arc::clone(&runtime));
    accept_loop(listener, runtime);
}

#[cfg(feature = "gtk-renderer")]
fn run_gtk_daemon(
    context: DaemonContext,
    listener: UnixListener,
    mut renderer_updates: Vec<mpsc::Sender<StaticRenderSyncPlan>>,
) -> Result<(), String> {
    use gtk::prelude::*;

    let (renderer_sender, renderer_receiver) = mpsc::channel::<StaticRenderSyncPlan>();
    renderer_updates.push(renderer_sender);
    let runtime = Arc::new(DaemonRuntime::new(context, renderer_updates));
    spawn_accept_loop(listener, Arc::clone(&runtime));

    let renderer = gilder::renderer::gtk::GtkStaticRenderer::new("io.github.elelcode.Gilder");
    let application = renderer.application().clone();
    let renderer = Rc::new(RefCell::new(renderer));
    let receiver = Rc::new(RefCell::new(Some(renderer_receiver)));
    let timers_installed = Rc::new(std::cell::Cell::new(false));

    let runtime_for_activate = Arc::clone(&runtime);
    let renderer_for_activate = Rc::clone(&renderer);
    let receiver_for_activate = Rc::clone(&receiver);
    let timers_for_activate = Rc::clone(&timers_installed);
    application.connect_activate(move |_| {
        match refreshed_render_sync(&runtime_for_activate) {
            Ok(sync) => {
                renderer_for_activate
                    .borrow_mut()
                    .sync_static_render_plan(&sync);
                runtime_for_activate.store_last_render_sync(sync);
            }
            Err(err) => eprintln!("gilderd: failed to prepare initial render sync: {err}"),
        }

        if timers_for_activate.replace(true) {
            return;
        }

        if let Some(receiver) = receiver_for_activate.borrow_mut().take() {
            let renderer_for_updates = Rc::clone(&renderer_for_activate);
            gtk::glib::timeout_add_local(Duration::from_millis(50), move || {
                while let Ok(sync) = receiver.try_recv() {
                    renderer_for_updates
                        .borrow_mut()
                        .sync_static_render_plan(&sync);
                }
                #[cfg(feature = "video-renderer")]
                renderer_for_updates.borrow_mut().poll_video_buses();
                gtk::glib::ControlFlow::Continue
            });
        }

        let runtime_for_refresh = Arc::clone(&runtime_for_activate);
        let refresh_interval = runtime_desktop_refresh_interval(&runtime_for_activate);
        gtk::glib::timeout_add_local(refresh_interval, move || {
            match refresh_runtime_desktop_if_changed(&runtime_for_refresh) {
                Ok(()) => {}
                Err(err) => eprintln!("gilderd: failed to refresh desktop state: {err}"),
            }
            gtk::glib::ControlFlow::Continue
        });
    });

    let _hold = application.hold();
    let exit_code = application.run();
    if exit_code == gtk::glib::ExitCode::SUCCESS {
        Ok(())
    } else {
        Err(format!(
            "GTK application exited with status {}",
            exit_code.get()
        ))
    }
}

#[cfg(feature = "gtk-renderer")]
fn spawn_accept_loop(listener: UnixListener, runtime: Arc<DaemonRuntime>) {
    thread::spawn(move || accept_loop(listener, runtime));
}

#[cfg(not(feature = "gtk-renderer"))]
fn spawn_desktop_refresh_loop(runtime: Arc<DaemonRuntime>) {
    thread::spawn(move || {
        loop {
            thread::sleep(runtime_desktop_refresh_interval(&runtime));
            if let Err(err) = refresh_runtime_desktop_if_changed(&runtime) {
                eprintln!("gilderd: failed to refresh desktop state: {err}");
            }
        }
    });
}

#[cfg(all(feature = "video-renderer", not(feature = "gtk-renderer")))]
fn spawn_video_renderer_loop(receiver: mpsc::Receiver<StaticRenderSyncPlan>) {
    thread::spawn(move || {
        let mut renderer = match gilder::renderer::video::GstVideoRenderer::new() {
            Ok(renderer) => renderer,
            Err(err) => {
                eprintln!("gilderd: failed to initialize video renderer: {err}");
                return;
            }
        };

        loop {
            match receiver.recv_timeout(std::time::Duration::from_millis(50)) {
                Ok(mut sync) => {
                    while let Ok(newer_sync) = receiver.try_recv() {
                        sync = newer_sync;
                    }
                    if let Err(err) = renderer.sync_render_plan(&sync) {
                        eprintln!("gilderd: failed to sync video renderer: {err}");
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }

            if let Err(err) = renderer.poll_bus() {
                eprintln!("gilderd: video renderer pipeline error: {err}");
            }
        }
    });
}

fn accept_loop(listener: UnixListener, runtime: Arc<DaemonRuntime>) {
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
            if let Some(render_sync) = outcome.render_sync {
                runtime.queue_render_sync_if_changed(render_sync);
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
            "events": ["snapshot", "desktop.changed", "state.changed"],
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
    renderer_updates: Vec<mpsc::Sender<StaticRenderSyncPlan>>,
    last_render_sync: Mutex<Option<StaticRenderSyncPlan>>,
}

impl DaemonRuntime {
    fn new(
        context: DaemonContext,
        renderer_updates: Vec<mpsc::Sender<StaticRenderSyncPlan>>,
    ) -> Self {
        Self {
            context: Mutex::new(context),
            watchers: WatchHub::new(),
            renderer_updates,
            last_render_sync: Mutex::new(None),
        }
    }

    fn lock_context(&self) -> Result<std::sync::MutexGuard<'_, DaemonContext>, String> {
        self.context
            .lock()
            .map_err(|_| "daemon context lock poisoned".to_owned())
    }

    fn queue_render_sync_if_changed(&self, render_sync: StaticRenderSyncPlan) -> bool {
        let Ok(mut last_render_sync) = self.last_render_sync.lock() else {
            eprintln!("gilderd: render sync cache lock poisoned");
            self.send_render_sync(render_sync);
            return true;
        };
        if last_render_sync.as_ref() == Some(&render_sync) {
            return false;
        }
        *last_render_sync = Some(render_sync.clone());
        drop(last_render_sync);
        self.send_render_sync(render_sync);
        true
    }

    fn store_last_render_sync(&self, render_sync: StaticRenderSyncPlan) {
        let Ok(mut last_render_sync) = self.last_render_sync.lock() else {
            eprintln!("gilderd: render sync cache lock poisoned");
            return;
        };
        *last_render_sync = Some(render_sync);
    }

    fn send_render_sync(&self, render_sync: StaticRenderSyncPlan) {
        for sender in &self.renderer_updates {
            if sender.send(render_sync.clone()).is_err() {
                eprintln!("gilderd: renderer update queue is closed");
            }
        }
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
    render_sync: Option<StaticRenderSyncPlan>,
}

impl IpcOutcome {
    fn response(response: String) -> Self {
        Self {
            response,
            event: None,
            render_sync: None,
        }
    }

    fn with_render_sync(response: String, event: Value, render_sync: StaticRenderSyncPlan) -> Self {
        Self {
            response,
            event: Some(event),
            render_sync: Some(render_sync),
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
                    "render_sync": render_sync_report(context),
                    "renderer": renderer_name(),
                    "renderer_capabilities": renderer_capabilities(),
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
                let render_sync = current_render_sync(context);
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
                let event =
                    state_changed_event("properties.set", output.as_deref(), context, &render_sync);
                IpcOutcome::with_render_sync(response, event, render_sync)
            }
        }
        RequestMethod::PropertiesUnset { output, key } => {
            let removed = context.state.unset_property(output.as_deref(), &key);
            if let Some(response) = persist_or_error(&request.id, context) {
                IpcOutcome::response(response)
            } else {
                refresh_desktop(context);
                let render_sync = current_render_sync(context);
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
                let event = state_changed_event(
                    "properties.unset",
                    output.as_deref(),
                    context,
                    &render_sync,
                );
                IpcOutcome::with_render_sync(response, event, render_sync)
            }
        }
        RequestMethod::Set {
            wallpaper,
            output,
            variant,
        } => {
            context.state.set_wallpaper_with_variant(
                output.as_deref(),
                wallpaper.clone(),
                variant.clone(),
            );
            if let Some(response) = persist_or_error(&request.id, context) {
                IpcOutcome::response(response)
            } else {
                refresh_desktop(context);
                let render_sync = current_render_sync(context);
                let response = renderer_action_response(
                    &request.id,
                    "set",
                    json!({
                        "wallpaper": wallpaper,
                        "output": output,
                        "variant": variant,
                    }),
                    &render_sync,
                );
                let event = state_changed_event("set", output.as_deref(), context, &render_sync);
                IpcOutcome::with_render_sync(response, event, render_sync)
            }
        }
        RequestMethod::Pause { output } => {
            context.state.pause(output.as_deref(), true);
            if let Some(response) = persist_or_error(&request.id, context) {
                IpcOutcome::response(response)
            } else {
                refresh_desktop(context);
                let render_sync = current_render_sync(context);
                let response = renderer_action_response(
                    &request.id,
                    "pause",
                    json!({
                        "output": output,
                    }),
                    &render_sync,
                );
                let event = state_changed_event("pause", output.as_deref(), context, &render_sync);
                IpcOutcome::with_render_sync(response, event, render_sync)
            }
        }
        RequestMethod::Resume { output } => {
            context.state.pause(output.as_deref(), false);
            if let Some(response) = persist_or_error(&request.id, context) {
                IpcOutcome::response(response)
            } else {
                refresh_desktop(context);
                let render_sync = current_render_sync(context);
                let response = renderer_action_response(
                    &request.id,
                    "resume",
                    json!({
                        "output": output,
                    }),
                    &render_sync,
                );
                let event = state_changed_event("resume", output.as_deref(), context, &render_sync);
                IpcOutcome::with_render_sync(response, event, render_sync)
            }
        }
        RequestMethod::Stop { output } => {
            context.state.stop(output.as_deref());
            if let Some(response) = persist_or_error(&request.id, context) {
                IpcOutcome::response(response)
            } else {
                refresh_desktop(context);
                let render_sync = current_render_sync(context);
                let response = renderer_action_response(
                    &request.id,
                    "stop",
                    json!({
                        "output": output,
                    }),
                    &render_sync,
                );
                let event = state_changed_event("stop", output.as_deref(), context, &render_sync);
                IpcOutcome::with_render_sync(response, event, render_sync)
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
    #[cfg(feature = "gtk-renderer")]
    {
        if context.config.adapters.generic_wayland
            && !gilder::renderer::gtk::can_read_gdk_desktop_outputs()
        {
            let mut adapters = context.config.adapters.clone();
            adapters.generic_wayland = false;
            let snapshot = gilder::desktop::adapters::read_desktop_snapshot(&adapters);
            if snapshot.compositor.is_some() || !snapshot.outputs.is_empty() {
                context.desktop = snapshot;
            }
            return;
        }
    }

    context.desktop = gilder::desktop::adapters::read_desktop_snapshot(&context.config.adapters);
}

fn output_reports(context: &DaemonContext) -> Vec<serde_json::Value> {
    let mut names: Vec<String> = context
        .desktop
        .outputs
        .iter()
        .map(|output| output.name.clone())
        .chain(context.state.outputs.keys().cloned())
        .chain(context.config.outputs.keys().cloned())
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
            let performance_config = context.config.performance_for_output(&name);
            let performance = gilder::policy::decide_performance(
                &performance_config,
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
    let render_sync = current_render_sync(context);
    json!({
        "desktop": context.desktop,
        "outputs": output_reports(context),
        "persisted_state": context.state,
        "render_sync": render_sync,
        "renderer": renderer_name(),
        "renderer_capabilities": renderer_capabilities(),
    })
}

fn state_changed_event(
    action: &str,
    output: Option<&str>,
    context: &DaemonContext,
    render_sync: &StaticRenderSyncPlan,
) -> Value {
    json!({
        "action": action,
        "output": output,
        "desktop": context.desktop,
        "outputs": output_reports(context),
        "persisted_state": context.state,
        "render_sync": render_sync,
        "renderer_capabilities": renderer_capabilities(),
    })
}

fn renderer_action_response(
    id: &serde_json::Value,
    accepted_method: &str,
    accepted_params: serde_json::Value,
    render_sync: &StaticRenderSyncPlan,
) -> String {
    let mut result = json!({
        "accepted": true,
        "method": accepted_method,
        "params": accepted_params,
        "renderer": renderer_name(),
        "renderer_capabilities": renderer_capabilities(),
        "render_sync": render_sync,
    });
    if !cfg!(any(feature = "gtk-renderer", feature = "video-renderer")) {
        result["note"] =
            json!("renderer was built without gtk-renderer or video-renderer features");
    } else if cfg!(feature = "video-renderer") && !cfg!(feature = "gtk-renderer") {
        result["note"] = json!("video renderer enabled; static wallpapers need gtk-renderer");
    }
    gilder::ipc::success_response(id, result)
}

fn renderer_name() -> &'static str {
    match (
        cfg!(feature = "gtk-renderer"),
        cfg!(feature = "video-renderer"),
    ) {
        (true, true) => "gtk-layer-shell-static+gtk-gstreamer-video",
        (true, false) => "gtk-layer-shell-static",
        (false, true) => "gstreamer-video",
        (false, false) => "not-implemented",
    }
}

fn renderer_capabilities() -> Value {
    json!({
        "gtk": {
            "built": cfg!(feature = "gtk-renderer"),
            "layer_shell_background_windows": cfg!(feature = "gtk-renderer"),
        },
        "video": video_renderer_capabilities(),
    })
}

#[cfg(feature = "video-renderer")]
fn video_renderer_capabilities() -> Value {
    json!({
        "built": true,
        "gtk_surface_path": cfg!(all(feature = "gtk-renderer", feature = "video-renderer")),
        "headless_worker": cfg!(all(feature = "video-renderer", not(feature = "gtk-renderer"))),
        "requires_gtk4paintablesink_for_surface": cfg!(all(feature = "gtk-renderer", feature = "video-renderer")),
        "gstreamer": gilder::renderer::video::runtime_capabilities(),
    })
}

#[cfg(not(feature = "video-renderer"))]
fn video_renderer_capabilities() -> Value {
    json!({
        "built": false,
        "gtk_surface_path": false,
        "headless_worker": false,
        "requires_gtk4paintablesink_for_surface": false,
    })
}

fn render_sync_report(context: &DaemonContext) -> Value {
    json!(current_render_sync(context))
}

fn current_render_sync(context: &DaemonContext) -> StaticRenderSyncPlan {
    gilder::renderer::static_render_sync_plan_with_config(
        &context.config,
        &context.desktop,
        &context.state,
        &context.paths.cache_dir,
    )
}

fn refreshed_render_sync(runtime: &DaemonRuntime) -> Result<StaticRenderSyncPlan, String> {
    let mut context = runtime.lock_context()?;
    refresh_desktop(&mut context);
    Ok(current_render_sync(&context))
}

fn refresh_runtime_desktop_if_changed(runtime: &DaemonRuntime) -> Result<(), String> {
    let Some((event, render_sync)) = ({
        let mut context = runtime.lock_context()?;
        let previous_desktop = context.desktop.clone();
        refresh_desktop(&mut context);
        if context.desktop == previous_desktop {
            None
        } else {
            let render_sync = current_render_sync(&context);
            let event = desktop_changed_event(&context, &render_sync);
            Some((event, render_sync))
        }
    }) else {
        return Ok(());
    };

    runtime.queue_render_sync_if_changed(render_sync);
    runtime.watchers.broadcast("desktop.changed", event);
    Ok(())
}

fn desktop_changed_event(context: &DaemonContext, render_sync: &StaticRenderSyncPlan) -> Value {
    json!({
        "desktop": context.desktop,
        "outputs": output_reports(context),
        "persisted_state": context.state,
        "render_sync": render_sync,
        "renderer": renderer_name(),
        "renderer_capabilities": renderer_capabilities(),
    })
}

fn runtime_desktop_refresh_interval(runtime: &DaemonRuntime) -> Duration {
    match runtime.lock_context() {
        Ok(context) => desktop_refresh_interval(&context.config.performance),
        Err(err) => {
            eprintln!("gilderd: failed to read desktop refresh interval: {err}");
            desktop_refresh_interval(&PerformanceConfig::default())
        }
    }
}

fn desktop_refresh_interval(config: &PerformanceConfig) -> Duration {
    Duration::from_millis(config.desktop_refresh_interval_ms.max(250))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_sync_dedup_tracks_last_queued_plan() {
        let runtime = DaemonRuntime::new(test_context(), Vec::new());
        let first = empty_render_sync();
        let second = StaticRenderSyncPlan {
            removals: vec!["eDP-1".to_owned()],
            ..empty_render_sync()
        };

        assert!(runtime.queue_render_sync_if_changed(first.clone()));
        assert!(!runtime.queue_render_sync_if_changed(first));
        assert!(runtime.queue_render_sync_if_changed(second));
    }

    #[test]
    fn render_sync_dedup_suppresses_repeated_renderer_updates() {
        let (sender, receiver) = mpsc::channel();
        let runtime = DaemonRuntime::new(test_context(), vec![sender]);
        let first = empty_render_sync();
        let second = StaticRenderSyncPlan {
            removals: vec!["eDP-1".to_owned()],
            ..empty_render_sync()
        };

        runtime.store_last_render_sync(first.clone());
        assert!(!runtime.queue_render_sync_if_changed(first.clone()));
        assert!(receiver.try_recv().is_err());

        assert!(runtime.queue_render_sync_if_changed(second.clone()));
        assert_eq!(receiver.try_recv().ok(), Some(second.clone()));
        assert!(!runtime.queue_render_sync_if_changed(second));
    }

    #[test]
    fn clamps_desktop_refresh_interval() {
        let config = PerformanceConfig {
            desktop_refresh_interval_ms: 0,
            ..PerformanceConfig::default()
        };
        assert_eq!(
            desktop_refresh_interval(&config),
            Duration::from_millis(250)
        );

        let config = PerformanceConfig {
            desktop_refresh_interval_ms: 1250,
            ..PerformanceConfig::default()
        };
        assert_eq!(
            desktop_refresh_interval(&config),
            Duration::from_millis(1250)
        );
    }

    #[test]
    fn output_reports_apply_output_performance_override() {
        let mut context = test_context();
        context.config.outputs.insert(
            "eDP-1".to_owned(),
            gilder::config::OutputConfig {
                performance: gilder::config::OutputPerformanceConfig {
                    interactive_max_fps: Some(42),
                    ..gilder::config::OutputPerformanceConfig::default()
                },
                ..gilder::config::OutputConfig::default()
            },
        );
        let reports = output_reports(&context);

        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0]["name"], json!("eDP-1"));
        assert_eq!(reports[0]["performance"]["mode"], json!("active"));
        assert_eq!(reports[0]["performance"]["max_fps"], json!(42));
        assert_eq!(reports[0]["performance"]["reason"], json!("interactive"));
    }

    fn test_context() -> DaemonContext {
        DaemonContext {
            paths: ApplicationPaths {
                config_file: PathBuf::from("/tmp/gilder-test/config.toml"),
                state_file: PathBuf::from("/tmp/gilder-test/state.json"),
                cache_dir: PathBuf::from("/tmp/gilder-test/cache"),
                data_dir: PathBuf::from("/tmp/gilder-test/data"),
            },
            config: GilderConfig::default(),
            state: AppState::default(),
            desktop: gilder::desktop::DesktopSnapshot::default(),
        }
    }

    fn empty_render_sync() -> StaticRenderSyncPlan {
        StaticRenderSyncPlan {
            plans: Vec::new(),
            video_plans: Vec::new(),
            removals: Vec::new(),
            errors: Vec::new(),
            decisions: Vec::new(),
        }
    }
}
