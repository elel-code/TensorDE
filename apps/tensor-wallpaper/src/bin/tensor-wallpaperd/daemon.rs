use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use tensor_wallpaper::config::{
    ApplicationPaths, CacheConfig, DynamicPausePolicy, TensorWallpaperConfig, OutputConfig,
    PerformanceConfig, PowerPolicy, ThrottlePolicy, VideoDecoderPolicy,
};
use tensor_wallpaper::ipc::RequestMethod;
use tensor_wallpaper::renderer::StaticRenderSyncPlan;
use tensor_wallpaper::state::{AppState, WallpaperAssignment};
use serde_json::{Map, Value, json};

#[path = "daemon/render_sync_telemetry.rs"]
mod render_sync_telemetry;
#[path = "daemon/renderer_runtime_report.rs"]
mod renderer_runtime_report;

use render_sync_telemetry::render_sync_telemetry_report;
use renderer_runtime_report::{renderer_runtime_report, renderer_telemetry_report};

fn main() {
    if let Err(err) = run() {
        eprintln!("tensor-wallpaperd: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let context = load_daemon_context()?;
    let listener = bind_ipc_listener()?;
    let renderer_runtime = Arc::new(Mutex::new(RendererRuntimeSnapshot::default()));
    let renderer_updates = renderer_update_senders(Arc::clone(&renderer_runtime));

    run_ipc_daemon(context, listener, renderer_updates, renderer_runtime);
    Ok(())
}

fn load_daemon_context() -> Result<DaemonContext, String> {
    let paths = ApplicationPaths::from_env().map_err(|err| err.to_string())?;
    let config = TensorWallpaperConfig::load(&paths.config_file)
        .map_err(|err| format!("failed to load {}: {err}", paths.config_file.display()))?;
    let state = tensor_wallpaper::state::load_state(&paths.state_file)
        .map_err(|err| format!("failed to load {}: {err}", paths.state_file.display()))?;
    let desktop = tensor_wallpaper::desktop::adapters::read_desktop_snapshot(&config.adapters);
    Ok(DaemonContext {
        paths,
        config,
        state,
        desktop,
        adaptive_monitor: tensor_wallpaper::adaptive::AdaptiveMonitor::default(),
        adaptive_snapshot: tensor_wallpaper::adaptive::AdaptiveSnapshot::default(),
        last_desktop_refresh: Some(Instant::now()),
        render_sync_cache: None,
        telemetry: DaemonTelemetry::default(),
    })
}

fn bind_ipc_listener() -> Result<UnixListener, String> {
    let socket = tensor_wallpaper::ipc::runtime_socket_path().ok_or_else(|| {
        "XDG_RUNTIME_DIR is not set; cannot create Wayland-session IPC".to_owned()
    })?;

    prepare_socket_parent(&socket)?;
    if socket.exists() {
        if UnixStream::connect(&socket).is_ok() {
            return Err(format!(
                "another tensor-wallpaperd instance is already listening on {}",
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
    eprintln!("tensor-wallpaperd: listening on {}", socket.display());

    Ok(listener)
}

fn renderer_update_senders(
    _renderer_runtime: Arc<Mutex<RendererRuntimeSnapshot>>,
) -> Vec<mpsc::Sender<StaticRenderSyncPlan>> {
    Vec::new()
}

fn run_ipc_daemon(
    context: DaemonContext,
    listener: UnixListener,
    renderer_updates: Vec<mpsc::Sender<StaticRenderSyncPlan>>,
    renderer_runtime: Arc<Mutex<RendererRuntimeSnapshot>>,
) {
    let runtime = Arc::new(DaemonRuntime::new(
        context,
        renderer_updates,
        renderer_runtime,
    ));
    match refreshed_render_sync(&runtime) {
        Ok(sync) => {
            runtime.queue_render_sync_if_changed(sync);
        }
        Err(err) => eprintln!("tensor-wallpaperd: failed to prepare initial render sync: {err}"),
    }
    spawn_desktop_refresh_loop(Arc::clone(&runtime));
    accept_loop(listener, runtime);
}

fn spawn_desktop_refresh_loop(runtime: Arc<DaemonRuntime>) {
    thread::spawn(move || {
        loop {
            thread::sleep(runtime_desktop_refresh_interval(&runtime));
            if let Err(err) = refresh_runtime_desktop_if_changed(&runtime) {
                eprintln!("tensor-wallpaperd: failed to refresh desktop state: {err}");
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
                        eprintln!("tensor-wallpaperd: client error: {err}");
                    }
                });
            }
            Err(err) => eprintln!("tensor-wallpaperd: failed to accept client: {err}"),
        }
    }
}

fn prepare_socket_parent(socket: &Path) -> Result<(), String> {
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

    let request = match tensor_wallpaper::ipc::parse_request(&request) {
        Ok(request) => request,
        Err(err) => {
            let response = tensor_wallpaper::ipc::error_response(err.id.as_ref(), err.code, &err.message);
            return write_line(&mut stream, &response);
        }
    };

    match request.method {
        RequestMethod::Watch { include_snapshot } => {
            handle_watch_client(stream, request.id, include_snapshot, runtime)
        }
        method => {
            let runtime_telemetry = runtime.telemetry_snapshot();
            let renderer_runtime = runtime.renderer_runtime_snapshot();
            let outcome = {
                let mut context = runtime.lock_context()?;
                handle_ipc_request(
                    tensor_wallpaper::ipc::IpcRequest {
                        id: request.id,
                        method,
                    },
                    &mut context,
                    runtime_telemetry,
                    renderer_runtime,
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
    let response = tensor_wallpaper::ipc::success_response(
        &id,
        json!({
            "subscribed": true,
            "protocol": tensor_wallpaper::ipc::PROTOCOL_VERSION,
            "events": ["snapshot", "desktop.changed", "state.changed"],
        }),
    );
    write_line(&mut stream, &response)?;

    if include_snapshot {
        let event = {
            let mut context = runtime.lock_context()?;
            snapshot_event(
                &mut context,
                runtime.telemetry_snapshot(),
                runtime.renderer_runtime_snapshot(),
            )
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
    renderer_runtime: Arc<Mutex<RendererRuntimeSnapshot>>,
    last_render_sync: Mutex<Option<StaticRenderSyncPlan>>,
    render_sync_updates_queued: AtomicU64,
    render_sync_updates_skipped: AtomicU64,
}

impl DaemonRuntime {
    fn new(
        context: DaemonContext,
        renderer_updates: Vec<mpsc::Sender<StaticRenderSyncPlan>>,
        renderer_runtime: Arc<Mutex<RendererRuntimeSnapshot>>,
    ) -> Self {
        Self {
            context: Mutex::new(context),
            watchers: WatchHub::new(),
            renderer_updates,
            renderer_runtime,
            last_render_sync: Mutex::new(None),
            render_sync_updates_queued: AtomicU64::new(0),
            render_sync_updates_skipped: AtomicU64::new(0),
        }
    }

    fn lock_context(&self) -> Result<std::sync::MutexGuard<'_, DaemonContext>, String> {
        self.context
            .lock()
            .map_err(|_| "daemon context lock poisoned".to_owned())
    }

    fn queue_render_sync_if_changed(&self, render_sync: StaticRenderSyncPlan) -> bool {
        let Ok(mut last_render_sync) = self.last_render_sync.lock() else {
            eprintln!("tensor-wallpaperd: render sync cache lock poisoned");
            self.render_sync_updates_queued
                .fetch_add(1, Ordering::Relaxed);
            self.send_render_sync(render_sync);
            return true;
        };
        if last_render_sync.as_ref() == Some(&render_sync) {
            self.render_sync_updates_skipped
                .fetch_add(1, Ordering::Relaxed);
            return false;
        }
        *last_render_sync = Some(render_sync.clone());
        drop(last_render_sync);
        self.render_sync_updates_queued
            .fetch_add(1, Ordering::Relaxed);
        self.send_render_sync(render_sync);
        true
    }

    #[cfg(test)]
    fn store_last_render_sync(&self, render_sync: StaticRenderSyncPlan) {
        let Ok(mut last_render_sync) = self.last_render_sync.lock() else {
            eprintln!("tensor-wallpaperd: render sync cache lock poisoned");
            return;
        };
        *last_render_sync = Some(render_sync);
    }

    fn send_render_sync(&self, render_sync: StaticRenderSyncPlan) {
        for sender in &self.renderer_updates {
            if sender.send(render_sync.clone()).is_err() {
                eprintln!("tensor-wallpaperd: renderer update queue is closed");
            }
        }
    }

    fn renderer_runtime_snapshot(&self) -> RendererRuntimeSnapshot {
        match self.renderer_runtime.lock() {
            Ok(snapshot) => snapshot.clone(),
            Err(_) => {
                eprintln!("tensor-wallpaperd: renderer runtime snapshot lock poisoned");
                RendererRuntimeSnapshot::default()
            }
        }
    }

    fn telemetry_snapshot(&self) -> RuntimeTelemetrySnapshot {
        RuntimeTelemetrySnapshot {
            render_sync_updates_queued: self.render_sync_updates_queued.load(Ordering::Relaxed),
            render_sync_updates_skipped: self.render_sync_updates_skipped.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
struct RendererRuntimeSnapshot {
    output_windows: usize,
    static_surfaces: usize,
    static_picture_surfaces: usize,
    static_css_surfaces: usize,
    static_color_surfaces: usize,
    slideshow_surfaces: usize,
    video_surfaces: usize,
    static_surface_resource_references: usize,
    static_surface_resource_bytes: u64,
    static_surface_unique_resources: usize,
    static_surface_unique_resource_bytes: u64,
    static_surface_estimated_decoded_bytes: u64,
    slideshow_resource_references: usize,
    slideshow_resource_bytes: u64,
    slideshow_unique_resources: usize,
    slideshow_unique_resource_bytes: u64,
    video_shared_runtimes: usize,
    video_pipeline_source_references: usize,
    video_pipeline_source_reference_bytes: u64,
    video_pipeline_unique_sources: usize,
    video_pipeline_unique_source_bytes: u64,
    video_pipelines: Vec<Value>,
}

#[derive(Debug, Clone, Copy, Default)]
struct RuntimeTelemetrySnapshot {
    render_sync_updates_queued: u64,
    render_sync_updates_skipped: u64,
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
            eprintln!("tensor-wallpaperd: watch subscriber lock poisoned");
            return;
        };
        subscribers.retain(|subscriber| subscriber.send(line.clone()).is_ok());
    }

    fn event_line(&self, event_type: &str, payload: Value) -> String {
        let sequence = self.next_sequence.fetch_add(1, Ordering::Relaxed);
        tensor_wallpaper::ipc::event_notification(sequence, event_type, payload)
    }
}

struct DaemonContext {
    paths: ApplicationPaths,
    config: TensorWallpaperConfig,
    state: AppState,
    desktop: tensor_wallpaper::desktop::DesktopSnapshot,
    adaptive_monitor: tensor_wallpaper::adaptive::AdaptiveMonitor,
    adaptive_snapshot: tensor_wallpaper::adaptive::AdaptiveSnapshot,
    last_desktop_refresh: Option<Instant>,
    render_sync_cache: Option<RenderSyncCache>,
    telemetry: DaemonTelemetry,
}

#[derive(Debug, Clone, Default)]
struct DaemonTelemetry {
    desktop_refreshes: u64,
    desktop_refresh_skips: u64,
    desktop_changes: u64,
    adaptive_refreshes: u64,
    adaptive_refresh_skips: u64,
    render_sync_cache_hits: u64,
    render_sync_cache_misses: u64,
    render_archive_cache_evictions: u64,
    render_archive_cache_eviction_errors: u64,
}

#[derive(Debug, Clone)]
struct RenderSyncCache {
    key: RenderSyncCacheKey,
    render_sync: StaticRenderSyncPlan,
}

#[derive(Debug, Clone, PartialEq)]
struct RenderSyncCacheKey {
    config: RenderSyncConfigKey,
    state: RenderSyncStateKey,
    desktop: tensor_wallpaper::desktop::DesktopSnapshot,
    adaptive_affects_render_plan: bool,
    playlist_clock: Option<tensor_wallpaper::renderer::PlaylistClockCacheKey>,
    cache_dir: PathBuf,
    packages: Vec<PackageInputFingerprint>,
    bound_properties: Vec<RenderSyncBoundPropertyKey>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderSyncConfigKey {
    default_wallpaper: Option<String>,
    outputs: BTreeMap<String, OutputConfig>,
    adaptive: tensor_wallpaper::config::AdaptiveConfig,
    video_decoder: VideoDecoderPolicy,
    cache: CacheConfig,
    performance: RenderSyncPerformanceKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RenderSyncPerformanceKey {
    interactive_max_fps: u32,
    background_max_fps: u32,
    battery_max_fps: u32,
    fullscreen: ThrottlePolicy,
    hidden: DynamicPausePolicy,
    session: DynamicPausePolicy,
    unfocused: ThrottlePolicy,
    battery: PowerPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderSyncStateKey {
    default_wallpaper: Option<WallpaperAssignment>,
    outputs: BTreeMap<String, OutputRenderStateKey>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct OutputRenderStateKey {
    wallpaper: Option<WallpaperAssignment>,
    paused: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct RenderSyncBoundPropertyKey {
    output_name: String,
    property: String,
    value: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PackageInputFingerprint {
    path: String,
    package: MetadataFingerprint,
    manifest: Option<PackageManifestFingerprint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PackageManifestFingerprint {
    json: MetadataFingerprint,
    toml: MetadataFingerprint,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MetadataFingerprint {
    Available {
        is_dir: bool,
        is_file: bool,
        len: u64,
        modified: Option<SystemTime>,
    },
    Unavailable(String),
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

fn handle_ipc_request(
    request: tensor_wallpaper::ipc::IpcRequest,
    context: &mut DaemonContext,
    runtime_telemetry: RuntimeTelemetrySnapshot,
    renderer_runtime: RendererRuntimeSnapshot,
) -> IpcOutcome {
    match request.method {
        RequestMethod::Ping { protocol } => IpcOutcome::response(tensor_wallpaper::ipc::success_response(
            &request.id,
            json!({
                "ok": true,
                "daemon": "tensor-wallpaperd",
                "protocol": tensor_wallpaper::ipc::PROTOCOL_VERSION,
                "client_protocol": protocol,
            }),
        )),
        RequestMethod::Status => {
            refresh_desktop_if_stale(context);
            let render_sync = current_render_sync(context);
            let outputs = output_reports(context, Some(&render_sync));
            IpcOutcome::response(tensor_wallpaper::ipc::success_response(
                &request.id,
                json!({
                    "state": "idle",
                    "config_file": context.paths.config_file,
                    "state_file": context.paths.state_file,
                    "desktop": context.desktop,
                    "outputs": outputs,
                    "persisted_state": context.state,
                    "render_sync": render_sync,
                    "renderer": renderer_name(),
                    "renderer_capabilities": renderer_capabilities(),
                    "renderer_runtime": renderer_runtime_report(&renderer_runtime),
                    "telemetry": telemetry_report(context, runtime_telemetry, &renderer_runtime),
                }),
            ))
        }
        RequestMethod::Outputs => {
            refresh_desktop_if_stale(context);
            refresh_adaptive_if_stale(context);
            let render_sync = current_render_sync(context);
            let outputs = output_reports(context, Some(&render_sync));
            IpcOutcome::response(tensor_wallpaper::ipc::success_response(
                &request.id,
                json!({ "desktop": context.desktop, "outputs": outputs }),
            ))
        }
        RequestMethod::Watch { .. } => IpcOutcome::response(tensor_wallpaper::ipc::error_response(
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
            IpcOutcome::response(tensor_wallpaper::ipc::success_response(&request.id, result))
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
                let response = tensor_wallpaper::ipc::success_response(
                    &request.id,
                    json!({
                        "accepted": true,
                        "method": "properties.set",
                        "output": output,
                        "key": key,
                        "value": value,
                    }),
                );
                let event = state_changed_event(
                    "properties.set",
                    output.as_deref(),
                    context,
                    &render_sync,
                    runtime_telemetry,
                    renderer_runtime,
                );
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
                let response = tensor_wallpaper::ipc::success_response(
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
                    runtime_telemetry,
                    renderer_runtime,
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
                let event = state_changed_event(
                    "set",
                    output.as_deref(),
                    context,
                    &render_sync,
                    runtime_telemetry,
                    renderer_runtime,
                );
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
                let event = state_changed_event(
                    "pause",
                    output.as_deref(),
                    context,
                    &render_sync,
                    runtime_telemetry,
                    renderer_runtime,
                );
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
                let event = state_changed_event(
                    "resume",
                    output.as_deref(),
                    context,
                    &render_sync,
                    runtime_telemetry,
                    renderer_runtime,
                );
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
                let event = state_changed_event(
                    "stop",
                    output.as_deref(),
                    context,
                    &render_sync,
                    runtime_telemetry,
                    renderer_runtime,
                );
                IpcOutcome::with_render_sync(response, event, render_sync)
            }
        }
    }
}

include!("daemon/render_sync_runtime.rs");
