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
#[cfg(feature = "gtk-renderer")]
use std::{cell::RefCell, rc::Rc};

use gilder::config::{
    ApplicationPaths, GilderConfig, OutputConfig, PerformanceConfig, PowerPolicy, ThrottlePolicy,
    VideoDecoderPolicy,
};
use gilder::ipc::RequestMethod;
use gilder::renderer::StaticRenderSyncPlan;
use gilder::state::{AppState, WallpaperAssignment};
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
    let renderer_runtime = Arc::new(Mutex::new(RendererRuntimeSnapshot::default()));
    let renderer_updates = renderer_update_senders(Arc::clone(&renderer_runtime));

    #[cfg(feature = "gtk-renderer")]
    {
        run_gtk_daemon(context, listener, renderer_updates, renderer_runtime)
    }

    #[cfg(not(feature = "gtk-renderer"))]
    {
        run_ipc_daemon(context, listener, renderer_updates, renderer_runtime);
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
        adaptive_monitor: gilder::adaptive::AdaptiveMonitor::default(),
        adaptive_snapshot: gilder::adaptive::AdaptiveSnapshot::default(),
        last_desktop_refresh: Some(Instant::now()),
        render_sync_cache: None,
        telemetry: DaemonTelemetry::default(),
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

fn renderer_update_senders(
    renderer_runtime: Arc<Mutex<RendererRuntimeSnapshot>>,
) -> Vec<mpsc::Sender<StaticRenderSyncPlan>> {
    #[cfg(any(
        not(feature = "video-renderer"),
        all(feature = "video-renderer", feature = "gtk-renderer")
    ))]
    {
        let _ = renderer_runtime;
        Vec::new()
    }

    #[cfg(all(feature = "video-renderer", not(feature = "gtk-renderer")))]
    {
        let mut senders = Vec::new();

        let (sender, receiver) = mpsc::channel::<StaticRenderSyncPlan>();
        spawn_video_renderer_loop(receiver, renderer_runtime);
        senders.push(sender);

        senders
    }
}

#[cfg(not(feature = "gtk-renderer"))]
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
    renderer_runtime: Arc<Mutex<RendererRuntimeSnapshot>>,
) -> Result<(), String> {
    use gtk::prelude::*;

    let (renderer_sender, renderer_receiver) = mpsc::channel::<StaticRenderSyncPlan>();
    renderer_updates.push(renderer_sender);
    let runtime = Arc::new(DaemonRuntime::new(
        context,
        renderer_updates,
        renderer_runtime,
    ));
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
                runtime_for_activate.store_renderer_runtime_snapshot(renderer_runtime_snapshot(
                    &renderer_for_activate.borrow(),
                ));
                runtime_for_activate.store_last_render_sync(sync);
            }
            Err(err) => eprintln!("gilderd: failed to prepare initial render sync: {err}"),
        }

        if timers_for_activate.replace(true) {
            return;
        }

        if let Some(receiver) = receiver_for_activate.borrow_mut().take() {
            let renderer_for_updates = Rc::clone(&renderer_for_activate);
            let runtime_for_updates = Arc::clone(&runtime_for_activate);
            gtk::glib::timeout_add_local(Duration::from_millis(50), move || {
                while let Ok(sync) = receiver.try_recv() {
                    renderer_for_updates
                        .borrow_mut()
                        .sync_static_render_plan(&sync);
                }
                renderer_for_updates.borrow_mut().tick_slideshows();
                #[cfg(feature = "video-renderer")]
                renderer_for_updates.borrow_mut().poll_video_buses();
                runtime_for_updates.store_renderer_runtime_snapshot(renderer_runtime_snapshot(
                    &renderer_for_updates.borrow(),
                ));
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
fn spawn_video_renderer_loop(
    receiver: mpsc::Receiver<StaticRenderSyncPlan>,
    renderer_runtime: Arc<Mutex<RendererRuntimeSnapshot>>,
) {
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
                    store_renderer_runtime_snapshot(
                        &renderer_runtime,
                        RendererRuntimeSnapshot::from_video_pipeline_snapshots(renderer.snapshot()),
                    );
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }

            if let Err(err) = renderer.poll_bus() {
                eprintln!("gilderd: video renderer pipeline error: {err}");
            }
            store_renderer_runtime_snapshot(
                &renderer_runtime,
                RendererRuntimeSnapshot::from_video_pipeline_snapshots(renderer.snapshot()),
            );
        }
    });
}

#[cfg(all(feature = "gtk-renderer", feature = "video-renderer"))]
fn renderer_runtime_snapshot(
    renderer: &gilder::renderer::gtk::GtkStaticRenderer,
) -> RendererRuntimeSnapshot {
    RendererRuntimeSnapshot::from_video_pipeline_snapshots(renderer.snapshot())
}

#[cfg(all(feature = "gtk-renderer", not(feature = "video-renderer")))]
fn renderer_runtime_snapshot(
    _renderer: &gilder::renderer::gtk::GtkStaticRenderer,
) -> RendererRuntimeSnapshot {
    RendererRuntimeSnapshot::default()
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
            let runtime_telemetry = runtime.telemetry_snapshot();
            let renderer_runtime = runtime.renderer_runtime_snapshot();
            let outcome = {
                let mut context = runtime.lock_context()?;
                handle_ipc_request(
                    gilder::ipc::IpcRequest {
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
            eprintln!("gilderd: render sync cache lock poisoned");
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

    #[cfg(any(test, feature = "gtk-renderer"))]
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

    #[cfg(feature = "gtk-renderer")]
    fn store_renderer_runtime_snapshot(&self, snapshot: RendererRuntimeSnapshot) {
        store_renderer_runtime_snapshot(&self.renderer_runtime, snapshot);
    }

    fn renderer_runtime_snapshot(&self) -> RendererRuntimeSnapshot {
        match self.renderer_runtime.lock() {
            Ok(snapshot) => snapshot.clone(),
            Err(_) => {
                eprintln!("gilderd: renderer runtime snapshot lock poisoned");
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
    video_pipelines: Vec<Value>,
}

impl RendererRuntimeSnapshot {
    #[cfg(feature = "video-renderer")]
    fn from_video_pipeline_snapshots(
        snapshots: Vec<gilder::renderer::video::VideoPipelineSnapshot>,
    ) -> Self {
        Self {
            video_pipelines: snapshots
                .into_iter()
                .map(|snapshot| {
                    serde_json::to_value(snapshot).unwrap_or_else(|err| {
                        json!({
                            "serialization_error": err.to_string(),
                        })
                    })
                })
                .collect(),
        }
    }
}

#[cfg(any(feature = "video-renderer", feature = "gtk-renderer"))]
fn store_renderer_runtime_snapshot(
    renderer_runtime: &Arc<Mutex<RendererRuntimeSnapshot>>,
    snapshot: RendererRuntimeSnapshot,
) {
    let Ok(mut runtime) = renderer_runtime.lock() else {
        eprintln!("gilderd: renderer runtime snapshot lock poisoned");
        return;
    };
    *runtime = snapshot;
}

fn renderer_runtime_report(snapshot: &RendererRuntimeSnapshot) -> Value {
    json!({
        "video_pipelines": snapshot.video_pipelines,
    })
}

fn renderer_telemetry_report(snapshot: &RendererRuntimeSnapshot) -> Value {
    let mut video_qos_messages = 0_u64;
    let mut video_qos_dropped_max = None;
    let mut video_gtk_frame_clock_ticks = 0_u64;
    let mut video_gtk_frame_clock_before_paint_ticks = 0_u64;
    let mut video_gtk_frame_clock_update_ticks = 0_u64;
    let mut video_gtk_frame_clock_layout_ticks = 0_u64;
    let mut video_gtk_frame_clock_paint_ticks = 0_u64;
    let mut video_gtk_frame_clock_after_paint_ticks = 0_u64;
    let mut video_gtk_frame_clock_interval_us_max = None;
    let mut video_gtk_frame_clock_fps_x1000_max = None;
    let mut video_gtk_frame_timings_complete = 0_u64;
    let mut video_gtk_frame_timings_presentation_interval_us_max = None;
    let mut video_gtk_frame_timings_presentation_time_us_max = None;

    for pipeline in &snapshot.video_pipelines {
        let Some(frame_stats) = pipeline.get("frame_stats") else {
            continue;
        };
        video_qos_messages = video_qos_messages
            .saturating_add(json_u64(frame_stats, "qos_messages").unwrap_or_default());
        update_optional_max(
            &mut video_qos_dropped_max,
            json_u64(frame_stats, "qos_dropped_max"),
        );
        video_gtk_frame_clock_ticks = video_gtk_frame_clock_ticks
            .saturating_add(json_u64(frame_stats, "gtk_frame_clock_ticks").unwrap_or_default());
        video_gtk_frame_clock_before_paint_ticks = video_gtk_frame_clock_before_paint_ticks
            .saturating_add(
                json_u64(frame_stats, "gtk_frame_clock_before_paint_ticks").unwrap_or_default(),
            );
        video_gtk_frame_clock_update_ticks = video_gtk_frame_clock_update_ticks.saturating_add(
            json_u64(frame_stats, "gtk_frame_clock_update_ticks").unwrap_or_default(),
        );
        video_gtk_frame_clock_layout_ticks = video_gtk_frame_clock_layout_ticks.saturating_add(
            json_u64(frame_stats, "gtk_frame_clock_layout_ticks").unwrap_or_default(),
        );
        video_gtk_frame_clock_paint_ticks = video_gtk_frame_clock_paint_ticks.saturating_add(
            json_u64(frame_stats, "gtk_frame_clock_paint_ticks").unwrap_or_default(),
        );
        video_gtk_frame_clock_after_paint_ticks = video_gtk_frame_clock_after_paint_ticks
            .saturating_add(
                json_u64(frame_stats, "gtk_frame_clock_after_paint_ticks").unwrap_or_default(),
            );
        update_optional_max(
            &mut video_gtk_frame_clock_interval_us_max,
            json_u64(frame_stats, "gtk_frame_clock_interval_us_max"),
        );
        update_optional_max(
            &mut video_gtk_frame_clock_fps_x1000_max,
            json_u64(frame_stats, "gtk_frame_clock_fps_x1000_latest"),
        );
        video_gtk_frame_timings_complete = video_gtk_frame_timings_complete.saturating_add(
            json_u64(frame_stats, "gtk_frame_timings_complete").unwrap_or_default(),
        );
        update_optional_max(
            &mut video_gtk_frame_timings_presentation_interval_us_max,
            json_u64(
                frame_stats,
                "gtk_frame_timings_presentation_interval_us_max",
            ),
        );
        update_optional_max(
            &mut video_gtk_frame_timings_presentation_time_us_max,
            json_u64(frame_stats, "gtk_frame_timings_presentation_time_us_latest"),
        );
    }

    json!({
        "video_pipelines": snapshot.video_pipelines.len(),
        "video_qos_messages": video_qos_messages,
        "video_qos_dropped_max": video_qos_dropped_max,
        "video_gtk_frame_clock_ticks": video_gtk_frame_clock_ticks,
        "video_gtk_frame_clock_before_paint_ticks": video_gtk_frame_clock_before_paint_ticks,
        "video_gtk_frame_clock_update_ticks": video_gtk_frame_clock_update_ticks,
        "video_gtk_frame_clock_layout_ticks": video_gtk_frame_clock_layout_ticks,
        "video_gtk_frame_clock_paint_ticks": video_gtk_frame_clock_paint_ticks,
        "video_gtk_frame_clock_after_paint_ticks": video_gtk_frame_clock_after_paint_ticks,
        "video_gtk_frame_clock_interval_us_max": video_gtk_frame_clock_interval_us_max,
        "video_gtk_frame_clock_fps_x1000_max": video_gtk_frame_clock_fps_x1000_max,
        "video_gtk_frame_timings_complete": video_gtk_frame_timings_complete,
        "video_gtk_frame_timings_presentation_interval_us_max": video_gtk_frame_timings_presentation_interval_us_max,
        "video_gtk_frame_timings_presentation_time_us_max": video_gtk_frame_timings_presentation_time_us_max,
    })
}

fn json_u64(object: &Value, key: &str) -> Option<u64> {
    object.get(key).and_then(Value::as_u64)
}

fn update_optional_max(slot: &mut Option<u64>, value: Option<u64>) {
    let Some(value) = value else {
        return;
    };
    *slot = Some(slot.map_or(value, |current| current.max(value)));
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
    adaptive_monitor: gilder::adaptive::AdaptiveMonitor,
    adaptive_snapshot: gilder::adaptive::AdaptiveSnapshot,
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
    desktop: gilder::desktop::DesktopSnapshot,
    adaptive_affects_render_plan: bool,
    cache_dir: PathBuf,
    packages: Vec<PackageInputFingerprint>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderSyncConfigKey {
    default_wallpaper: Option<String>,
    outputs: BTreeMap<String, OutputConfig>,
    adaptive: gilder::config::AdaptiveConfig,
    video_decoder: VideoDecoderPolicy,
    performance: RenderSyncPerformanceKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RenderSyncPerformanceKey {
    interactive_max_fps: u32,
    background_max_fps: u32,
    battery_max_fps: u32,
    fullscreen: ThrottlePolicy,
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
    request: gilder::ipc::IpcRequest,
    context: &mut DaemonContext,
    runtime_telemetry: RuntimeTelemetrySnapshot,
    renderer_runtime: RendererRuntimeSnapshot,
) -> IpcOutcome {
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
            refresh_desktop_if_stale(context);
            let render_sync = current_render_sync(context);
            IpcOutcome::response(gilder::ipc::success_response(
                &request.id,
                json!({
                    "state": "idle",
                    "config_file": context.paths.config_file,
                    "state_file": context.paths.state_file,
                    "desktop": context.desktop,
                    "outputs": output_reports(context),
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
            mark_desktop_refreshed(context);
            return;
        }
    }

    context.desktop = gilder::desktop::adapters::read_desktop_snapshot(&context.config.adapters);
    mark_desktop_refreshed(context);
}

fn refresh_desktop_if_stale(context: &mut DaemonContext) {
    let interval = desktop_refresh_interval(&context.config.performance);
    let is_stale = context
        .last_desktop_refresh
        .map(|last_refresh| last_refresh.elapsed() >= interval)
        .unwrap_or(true);
    if is_stale {
        refresh_desktop(context);
    } else {
        context.telemetry.desktop_refresh_skips += 1;
    }
}

fn refresh_adaptive_if_stale(context: &mut DaemonContext) {
    let interval = adaptive_refresh_interval(&context.config.adaptive);
    if context.adaptive_monitor.should_refresh(interval) {
        context.adaptive_snapshot = context.adaptive_monitor.refresh(&context.config);
        context.telemetry.adaptive_refreshes += 1;
    } else {
        context.telemetry.adaptive_refresh_skips += 1;
    }
}

fn mark_desktop_refreshed(context: &mut DaemonContext) {
    context.last_desktop_refresh = Some(Instant::now());
    context.telemetry.desktop_refreshes += 1;
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
            let performance = gilder::policy::apply_adaptive_policy(
                performance,
                &context.config,
                &name,
                desktop_output,
                &context.adaptive_snapshot,
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

fn snapshot_event(
    context: &mut DaemonContext,
    runtime_telemetry: RuntimeTelemetrySnapshot,
    renderer_runtime: RendererRuntimeSnapshot,
) -> Value {
    let render_sync = current_render_sync(context);
    json!({
        "desktop": context.desktop,
        "outputs": output_reports(context),
        "persisted_state": context.state,
        "render_sync": render_sync,
        "renderer": renderer_name(),
        "renderer_capabilities": renderer_capabilities(),
        "renderer_runtime": renderer_runtime_report(&renderer_runtime),
        "telemetry": telemetry_report(context, runtime_telemetry, &renderer_runtime),
    })
}

fn state_changed_event(
    action: &str,
    output: Option<&str>,
    context: &DaemonContext,
    render_sync: &StaticRenderSyncPlan,
    runtime_telemetry: RuntimeTelemetrySnapshot,
    renderer_runtime: RendererRuntimeSnapshot,
) -> Value {
    json!({
        "action": action,
        "output": output,
        "desktop": context.desktop,
        "outputs": output_reports(context),
        "persisted_state": context.state,
        "render_sync": render_sync,
        "renderer_capabilities": renderer_capabilities(),
        "renderer_runtime": renderer_runtime_report(&renderer_runtime),
        "telemetry": telemetry_report(context, runtime_telemetry, &renderer_runtime),
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

fn telemetry_report(
    context: &DaemonContext,
    runtime_telemetry: RuntimeTelemetrySnapshot,
    renderer_runtime: &RendererRuntimeSnapshot,
) -> Value {
    json!({
        "desktop": {
            "refreshes": context.telemetry.desktop_refreshes,
            "refresh_skips": context.telemetry.desktop_refresh_skips,
            "changes": context.telemetry.desktop_changes,
            "last_refresh_age_ms": context.last_desktop_refresh.map(elapsed_millis_u64),
        },
        "adaptive": {
            "refreshes": context.telemetry.adaptive_refreshes,
            "refresh_skips": context.telemetry.adaptive_refresh_skips,
            "snapshot": context.adaptive_snapshot,
            "action": adaptive_action_report(context),
        },
        "render_sync": {
            "cache_hits": context.telemetry.render_sync_cache_hits,
            "cache_misses": context.telemetry.render_sync_cache_misses,
            "updates_queued": runtime_telemetry.render_sync_updates_queued,
            "updates_skipped": runtime_telemetry.render_sync_updates_skipped,
        },
        "renderer": renderer_telemetry_report(renderer_runtime),
    })
}

fn adaptive_action_report(context: &DaemonContext) -> Value {
    if !context.adaptive_snapshot.affects_render_plan() {
        return Value::Null;
    }

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

    let actions = names
        .into_iter()
        .filter(|name| gilder::adaptive::output_enabled(&context.config, name))
        .map(|name| {
            let desktop_output = context.desktop.output(&name);
            match gilder::adaptive::output_action(&context.config, &name) {
                gilder::config::AdaptiveAction::PauseUnfocused
                    if desktop_output.is_some_and(|output| !output.focused) =>
                {
                    json!({
                        "output_name": name,
                        "type": "pause-unfocused",
                    })
                }
                gilder::config::AdaptiveAction::PauseDynamic => {
                    json!({
                        "output_name": name,
                        "type": "pause-dynamic",
                        "scope": "dynamic-wallpapers",
                    })
                }
                action => {
                    let max_fps = gilder::adaptive::output_throttle_max_fps(&context.config, &name);
                    json!({
                        "output_name": name,
                        "type": "throttle",
                        "configured_action": action,
                        "max_fps": max_fps,
                    })
                }
            }
        })
        .collect::<Vec<_>>();
    json!(actions)
}

fn elapsed_millis_u64(instant: Instant) -> u64 {
    instant.elapsed().as_millis().min(u64::MAX as u128) as u64
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

fn current_render_sync(context: &mut DaemonContext) -> StaticRenderSyncPlan {
    refresh_adaptive_if_stale(context);
    let key = render_sync_cache_key(context);
    if let Some(cache) = &context.render_sync_cache
        && cache.key == key
    {
        context.telemetry.render_sync_cache_hits += 1;
        return cache.render_sync.clone();
    }

    context.telemetry.render_sync_cache_misses += 1;
    let render_sync = gilder::renderer::static_render_sync_plan_with_config_and_adaptive(
        &context.config,
        &context.desktop,
        &context.state,
        &context.paths.cache_dir,
        &context.adaptive_snapshot,
    );
    context.render_sync_cache = Some(RenderSyncCache {
        key,
        render_sync: render_sync.clone(),
    });
    render_sync
}

fn render_sync_cache_key(context: &DaemonContext) -> RenderSyncCacheKey {
    RenderSyncCacheKey {
        config: render_sync_config_key(&context.config),
        state: render_sync_state_key(&context.state),
        desktop: context.desktop.clone(),
        adaptive_affects_render_plan: context.adaptive_snapshot.affects_render_plan(),
        cache_dir: context.paths.cache_dir.clone(),
        packages: wallpaper_package_fingerprints(context),
    }
}

fn render_sync_config_key(config: &GilderConfig) -> RenderSyncConfigKey {
    RenderSyncConfigKey {
        default_wallpaper: config.default_wallpaper.clone(),
        outputs: config.outputs.clone(),
        adaptive: config.adaptive.clone(),
        video_decoder: config.video.decoder,
        performance: RenderSyncPerformanceKey {
            interactive_max_fps: config.performance.interactive_max_fps,
            background_max_fps: config.performance.background_max_fps,
            battery_max_fps: config.performance.battery_max_fps,
            fullscreen: config.performance.fullscreen,
            unfocused: config.performance.unfocused,
            battery: config.performance.battery,
        },
    }
}

fn render_sync_state_key(state: &AppState) -> RenderSyncStateKey {
    RenderSyncStateKey {
        default_wallpaper: state.default_wallpaper.clone(),
        outputs: state
            .outputs
            .iter()
            .map(|(name, state)| {
                (
                    name.clone(),
                    OutputRenderStateKey {
                        wallpaper: state.wallpaper.clone(),
                        paused: state.paused,
                    },
                )
            })
            .collect(),
    }
}

fn wallpaper_package_fingerprints(context: &DaemonContext) -> Vec<PackageInputFingerprint> {
    let mut paths = Vec::new();
    if let Some(assignment) = &context.state.default_wallpaper {
        paths.push(assignment.path.clone());
    }
    paths.extend(context.state.outputs.values().filter_map(|state| {
        state
            .wallpaper
            .as_ref()
            .map(|assignment| assignment.path.clone())
    }));
    if let Some(path) = &context.config.default_wallpaper {
        paths.push(path.clone());
    }
    paths.extend(
        context
            .config
            .outputs
            .values()
            .filter_map(|output| output.wallpaper.clone()),
    );
    paths.sort();
    paths.dedup();

    paths
        .into_iter()
        .map(|path| PackageInputFingerprint::new(path))
        .collect()
}

impl PackageInputFingerprint {
    fn new(path: String) -> Self {
        let package_path = Path::new(&path);
        let package = metadata_fingerprint(package_path);
        let manifest = if package_path.is_dir()
            || package_path
                .extension()
                .and_then(|extension| extension.to_str())
                == Some("gwpdir")
        {
            Some(PackageManifestFingerprint {
                json: metadata_fingerprint(&package_path.join(gilder::core::MANIFEST_FILE)),
                toml: metadata_fingerprint(&package_path.join(gilder::core::MANIFEST_TOML_FILE)),
            })
        } else {
            None
        };
        Self {
            path,
            package,
            manifest,
        }
    }
}

fn metadata_fingerprint(path: &Path) -> MetadataFingerprint {
    match fs::metadata(path) {
        Ok(metadata) => MetadataFingerprint::Available {
            is_dir: metadata.is_dir(),
            is_file: metadata.is_file(),
            len: metadata.len(),
            modified: metadata.modified().ok(),
        },
        Err(err) => MetadataFingerprint::Unavailable(err.kind().to_string()),
    }
}

fn refreshed_render_sync(runtime: &DaemonRuntime) -> Result<StaticRenderSyncPlan, String> {
    let mut context = runtime.lock_context()?;
    refresh_desktop(&mut context);
    Ok(current_render_sync(&mut context))
}

fn refresh_runtime_desktop_if_changed(runtime: &DaemonRuntime) -> Result<(), String> {
    let Some((event_type, event, render_sync)) = ({
        let mut context = runtime.lock_context()?;
        let previous_desktop = context.desktop.clone();
        let previous_adaptive_affects_render_plan = context.adaptive_snapshot.affects_render_plan();
        refresh_desktop(&mut context);
        refresh_adaptive_if_stale(&mut context);
        let desktop_changed = context.desktop != previous_desktop;
        let adaptive_affects_render_plan_changed = context.adaptive_snapshot.affects_render_plan()
            != previous_adaptive_affects_render_plan;

        if !desktop_changed && !adaptive_affects_render_plan_changed {
            None
        } else {
            if desktop_changed {
                context.telemetry.desktop_changes += 1;
            }
            let render_sync = current_render_sync(&mut context);
            let event = runtime_changed_event(
                &context,
                &render_sync,
                runtime.telemetry_snapshot(),
                runtime.renderer_runtime_snapshot(),
            );
            let event_type = if desktop_changed {
                "desktop.changed"
            } else {
                "adaptive.changed"
            };
            Some((event_type, event, render_sync))
        }
    }) else {
        return Ok(());
    };

    runtime.queue_render_sync_if_changed(render_sync);
    runtime.watchers.broadcast(event_type, event);
    Ok(())
}

fn runtime_changed_event(
    context: &DaemonContext,
    render_sync: &StaticRenderSyncPlan,
    runtime_telemetry: RuntimeTelemetrySnapshot,
    renderer_runtime: RendererRuntimeSnapshot,
) -> Value {
    json!({
        "desktop": context.desktop,
        "outputs": output_reports(context),
        "persisted_state": context.state,
        "render_sync": render_sync,
        "renderer": renderer_name(),
        "renderer_capabilities": renderer_capabilities(),
        "renderer_runtime": renderer_runtime_report(&renderer_runtime),
        "telemetry": telemetry_report(context, runtime_telemetry, &renderer_runtime),
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

fn adaptive_refresh_interval(config: &gilder::config::AdaptiveConfig) -> Duration {
    Duration::from_millis(config.refresh_interval_ms.max(250))
}

fn desktop_refresh_interval(config: &PerformanceConfig) -> Duration {
    Duration::from_millis(config.desktop_refresh_interval_ms.max(250))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_sync_dedup_tracks_last_queued_plan() {
        let runtime = test_runtime(test_context(), Vec::new());
        let first = empty_render_sync();
        let second = StaticRenderSyncPlan {
            removals: vec!["eDP-1".to_owned()],
            ..empty_render_sync()
        };

        assert!(runtime.queue_render_sync_if_changed(first.clone()));
        assert!(!runtime.queue_render_sync_if_changed(first));
        assert!(runtime.queue_render_sync_if_changed(second));
        let telemetry = runtime.telemetry_snapshot();
        assert_eq!(telemetry.render_sync_updates_queued, 2);
        assert_eq!(telemetry.render_sync_updates_skipped, 1);
    }

    #[test]
    fn render_sync_dedup_suppresses_repeated_renderer_updates() {
        let (sender, receiver) = mpsc::channel();
        let runtime = test_runtime(test_context(), vec![sender]);
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
    fn read_requests_refresh_desktop_only_after_interval() {
        let mut context = test_context();
        context.config.adapters = gilder::config::AdapterConfig {
            generic_wayland: false,
            hyprland: false,
            niri: false,
        };
        context.config.performance.desktop_refresh_interval_ms = 1_000;
        context.desktop.outputs = vec![gilder::desktop::DesktopOutput::virtual_output("eDP-1")];
        context.last_desktop_refresh = Some(Instant::now());

        refresh_desktop_if_stale(&mut context);
        assert_eq!(context.desktop.outputs.len(), 1);
        assert_eq!(context.telemetry.desktop_refresh_skips, 1);
        assert_eq!(context.telemetry.desktop_refreshes, 0);

        context.last_desktop_refresh = Some(Instant::now() - Duration::from_millis(1_500));
        refresh_desktop_if_stale(&mut context);
        assert!(context.desktop.outputs.is_empty());
        assert!(context.last_desktop_refresh.is_some());
        assert_eq!(context.telemetry.desktop_refresh_skips, 1);
        assert_eq!(context.telemetry.desktop_refreshes, 1);
    }

    #[test]
    fn status_response_reports_daemon_telemetry() {
        let mut context = test_context();
        let request = gilder::ipc::IpcRequest {
            id: json!(1),
            method: RequestMethod::Status,
        };

        let renderer_runtime = RendererRuntimeSnapshot {
            video_pipelines: vec![
                json!({
                    "output_name": "eDP-1",
                    "actual_decoders": ["dav1ddec"],
                    "frame_stats": {
                        "qos_messages": 3,
                        "qos_dropped_max": 2,
                        "gtk_frame_clock_ticks": 9,
                        "gtk_frame_clock_before_paint_ticks": 8,
                        "gtk_frame_clock_update_ticks": 7,
                        "gtk_frame_clock_layout_ticks": 6,
                        "gtk_frame_clock_paint_ticks": 5,
                        "gtk_frame_clock_after_paint_ticks": 9,
                        "gtk_frame_clock_interval_us_max": 20000,
                        "gtk_frame_clock_fps_x1000_latest": 59940,
                        "gtk_frame_timings_complete": 5,
                        "gtk_frame_timings_presentation_interval_us_max": 21000,
                        "gtk_frame_timings_presentation_time_us_latest": 100000,
                    },
                }),
                json!({
                    "output_name": "HDMI-A-1",
                    "actual_decoders": ["vaav1dec"],
                    "frame_stats": {
                        "qos_messages": 4,
                        "qos_dropped_max": 3,
                        "gtk_frame_clock_ticks": 31,
                        "gtk_frame_clock_before_paint_ticks": 20,
                        "gtk_frame_clock_update_ticks": 21,
                        "gtk_frame_clock_layout_ticks": 22,
                        "gtk_frame_clock_paint_ticks": 23,
                        "gtk_frame_clock_after_paint_ticks": 31,
                        "gtk_frame_clock_interval_us_max": 18000,
                        "gtk_frame_clock_fps_x1000_latest": 60000,
                        "gtk_frame_timings_complete": 7,
                        "gtk_frame_timings_presentation_interval_us_max": 19000,
                        "gtk_frame_timings_presentation_time_us_latest": 150000,
                    },
                }),
            ],
        };
        let outcome = handle_ipc_request(
            request,
            &mut context,
            RuntimeTelemetrySnapshot::default(),
            renderer_runtime,
        );
        let response: serde_json::Value =
            serde_json::from_str(&outcome.response).expect("status response should be JSON");

        assert_eq!(
            response["result"]["telemetry"]["desktop"]["refresh_skips"],
            json!(1)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["cache_misses"],
            json!(1)
        );
        assert_eq!(
            response["result"]["telemetry"]["render_sync"]["updates_queued"],
            json!(0)
        );
        assert!(
            response["result"]["telemetry"]["desktop"]["last_refresh_age_ms"]
                .as_u64()
                .is_some()
        );
        assert_eq!(
            response["result"]["renderer_runtime"]["video_pipelines"][0]["actual_decoders"],
            json!(["dav1ddec"])
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["video_pipelines"],
            json!(2)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["video_qos_messages"],
            json!(7)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["video_qos_dropped_max"],
            json!(3)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["video_gtk_frame_clock_ticks"],
            json!(40)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["video_gtk_frame_clock_before_paint_ticks"],
            json!(28)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["video_gtk_frame_clock_update_ticks"],
            json!(28)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["video_gtk_frame_clock_layout_ticks"],
            json!(28)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["video_gtk_frame_clock_paint_ticks"],
            json!(28)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["video_gtk_frame_clock_after_paint_ticks"],
            json!(40)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["video_gtk_frame_clock_interval_us_max"],
            json!(20000)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["video_gtk_frame_clock_fps_x1000_max"],
            json!(60000)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["video_gtk_frame_timings_complete"],
            json!(12)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["video_gtk_frame_timings_presentation_interval_us_max"],
            json!(21000)
        );
        assert_eq!(
            response["result"]["telemetry"]["renderer"]["video_gtk_frame_timings_presentation_time_us_max"],
            json!(150000)
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

    #[test]
    fn output_reports_apply_adaptive_throttle() {
        let mut context = test_context();
        context.desktop.outputs = vec![gilder::desktop::DesktopOutput::virtual_output("eDP-1")];
        context.config.adaptive.enabled = true;
        context.config.adaptive.throttle_max_fps = 15;
        context.adaptive_snapshot = gilder::adaptive::AdaptiveSnapshot {
            monitoring_enabled: true,
            active_triggers: vec![gilder::adaptive::AdaptiveTrigger {
                metric: gilder::adaptive::AdaptiveMetric::CpuPressureSomeAvg10,
                value_x100: 9_000,
                threshold_x100: 7_500,
            }],
            ..gilder::adaptive::AdaptiveSnapshot::default()
        };
        let reports = output_reports(&context);

        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0]["performance"]["mode"], json!("throttled"));
        assert_eq!(reports[0]["performance"]["max_fps"], json!(15));
        assert_eq!(reports[0]["performance"]["reason"], json!("adaptive"));
    }

    #[test]
    fn output_reports_apply_adaptive_pause_unfocused() {
        let mut context = test_context();
        context.desktop.outputs = vec![gilder::desktop::DesktopOutput {
            focused: false,
            ..gilder::desktop::DesktopOutput::virtual_output("eDP-1")
        }];
        context.config.adaptive.enabled = true;
        context.config.adaptive.action = gilder::config::AdaptiveAction::PauseUnfocused;
        context.adaptive_snapshot = gilder::adaptive::AdaptiveSnapshot {
            monitoring_enabled: true,
            active_triggers: vec![gilder::adaptive::AdaptiveTrigger {
                metric: gilder::adaptive::AdaptiveMetric::CpuPressureSomeAvg10,
                value_x100: 9_000,
                threshold_x100: 7_500,
            }],
            ..gilder::adaptive::AdaptiveSnapshot::default()
        };
        let reports = output_reports(&context);
        let actions = adaptive_action_report(&context);

        assert_eq!(reports.len(), 1);
        assert_eq!(reports[0]["performance"]["mode"], json!("paused"));
        assert_eq!(reports[0]["performance"]["max_fps"], Value::Null);
        assert_eq!(reports[0]["performance"]["reason"], json!("adaptive"));
        assert_eq!(actions[0]["type"], json!("pause-unfocused"));
        assert_eq!(actions[0]["max_fps"], Value::Null);
    }

    #[test]
    fn adaptive_action_report_reports_pause_dynamic_scope() {
        let mut context = test_context();
        context.desktop.outputs = vec![gilder::desktop::DesktopOutput::virtual_output("eDP-1")];
        context.config.adaptive.enabled = true;
        context.config.adaptive.action = gilder::config::AdaptiveAction::PauseDynamic;
        context.adaptive_snapshot = gilder::adaptive::AdaptiveSnapshot {
            monitoring_enabled: true,
            active_triggers: vec![gilder::adaptive::AdaptiveTrigger {
                metric: gilder::adaptive::AdaptiveMetric::CpuPressureSomeAvg10,
                value_x100: 9_000,
                threshold_x100: 7_500,
            }],
            ..gilder::adaptive::AdaptiveSnapshot::default()
        };

        let actions = adaptive_action_report(&context);

        assert_eq!(actions[0]["type"], json!("pause-dynamic"));
        assert_eq!(actions[0]["scope"], json!("dynamic-wallpapers"));
        assert_eq!(actions[0]["max_fps"], Value::Null);
    }

    #[test]
    fn current_render_sync_cache_invalidates_when_manifest_changes() {
        let package_dir = TestDir::new("gilder-render-sync-cache-package");
        write_static_package_manifest(package_dir.path(), "#101418");

        let mut context = test_context();
        context.paths.cache_dir = package_dir.path().join("cache");
        context.desktop.outputs = vec![gilder::desktop::DesktopOutput::virtual_output("eDP-1")];
        context
            .state
            .set_wallpaper(None, package_dir.path().to_string_lossy());

        let first = current_render_sync(&mut context);
        assert_eq!(first.plans[0].background.as_deref(), Some("#101418"));
        assert!(context.render_sync_cache.is_some());

        let second = current_render_sync(&mut context);
        assert_eq!(second, first);
        assert_eq!(context.telemetry.render_sync_cache_hits, 1);
        assert_eq!(context.telemetry.render_sync_cache_misses, 1);

        write_static_package_manifest(package_dir.path(), "#203040ff");
        let third = current_render_sync(&mut context);
        assert_eq!(third.plans[0].background.as_deref(), Some("#203040ff"));
        assert_eq!(context.telemetry.render_sync_cache_hits, 1);
        assert_eq!(context.telemetry.render_sync_cache_misses, 2);
    }

    #[test]
    fn current_render_sync_cache_ignores_existing_output_properties() {
        let package_dir = TestDir::new("gilder-render-sync-property-cache-package");
        write_static_package_manifest(package_dir.path(), "#101418");

        let mut context = test_context();
        context.paths.cache_dir = package_dir.path().join("cache");
        context.desktop.outputs = vec![gilder::desktop::DesktopOutput::virtual_output("eDP-1")];
        context
            .state
            .set_wallpaper(Some("eDP-1"), package_dir.path().to_string_lossy());

        let cached = StaticRenderSyncPlan {
            removals: vec!["cached-plan".to_owned()],
            ..empty_render_sync()
        };
        context.render_sync_cache = Some(RenderSyncCache {
            key: render_sync_cache_key(&context),
            render_sync: cached.clone(),
        });

        context
            .state
            .set_property(Some("eDP-1"), "speed", json!(0.5));
        assert_eq!(current_render_sync(&mut context), cached);

        context.state.pause(Some("eDP-1"), true);
        let paused = current_render_sync(&mut context);
        assert_ne!(paused, cached);
        assert_eq!(paused.removals, vec!["eDP-1"]);
    }

    #[test]
    fn current_render_sync_cache_ignores_non_render_config() {
        let package_dir = TestDir::new("gilder-render-sync-config-cache-package");
        write_static_package_manifest(package_dir.path(), "#101418");

        let mut context = test_context();
        context.paths.cache_dir = package_dir.path().join("cache");
        context.desktop.outputs = vec![gilder::desktop::DesktopOutput::virtual_output("eDP-1")];
        context
            .state
            .set_wallpaper(Some("eDP-1"), package_dir.path().to_string_lossy());

        let cached = StaticRenderSyncPlan {
            removals: vec!["cached-plan".to_owned()],
            ..empty_render_sync()
        };
        context.render_sync_cache = Some(RenderSyncCache {
            key: render_sync_cache_key(&context),
            render_sync: cached.clone(),
        });

        context.config.adapters.niri = false;
        context.config.performance.desktop_refresh_interval_ms = 7_500;
        assert_eq!(current_render_sync(&mut context), cached);

        context.config.outputs.insert(
            "eDP-1".to_owned(),
            OutputConfig {
                fit: Some(gilder::core::FitMode::Contain),
                ..OutputConfig::default()
            },
        );
        let updated = current_render_sync(&mut context);
        assert_ne!(updated, cached);
        assert_eq!(updated.plans[0].fit, gilder::core::FitMode::Contain);
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
            adaptive_monitor: gilder::adaptive::AdaptiveMonitor::default(),
            adaptive_snapshot: gilder::adaptive::AdaptiveSnapshot::default(),
            last_desktop_refresh: Some(Instant::now()),
            render_sync_cache: None,
            telemetry: DaemonTelemetry::default(),
        }
    }

    fn test_runtime(
        context: DaemonContext,
        renderer_updates: Vec<mpsc::Sender<StaticRenderSyncPlan>>,
    ) -> DaemonRuntime {
        DaemonRuntime::new(
            context,
            renderer_updates,
            Arc::new(Mutex::new(RendererRuntimeSnapshot::default())),
        )
    }

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(name: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .expect("test clock is before Unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!("{name}-{}-{unique}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("failed to create test directory");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn write_static_package_manifest(root: &Path, background: &str) {
        let assets = root.join("assets");
        std::fs::create_dir_all(&assets).expect("failed to create package assets");
        std::fs::write(
            assets.join("wallpaper.svg"),
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="9"><rect width="16" height="9" fill="#101418"/></svg>"##,
        )
        .expect("failed to write package asset");
        std::fs::write(
            root.join(gilder::core::MANIFEST_FILE),
            format!(
                r#"{{
  "format": "gilder.wallpaper",
  "format_version": 1,
  "id": "io.github.elelcode.gilder.cache-test",
  "version": "0.1.0",
  "title": "Cache Test",
  "kind": "static-image",
  "entry": {{
    "type": "static-image",
    "source": "assets/wallpaper.svg",
    "fit": "cover",
    "background": "{background}"
  }}
}}
"#
            ),
        )
        .expect("failed to write package manifest");
    }

    fn empty_render_sync() -> StaticRenderSyncPlan {
        StaticRenderSyncPlan {
            plans: Vec::new(),
            video_plans: Vec::new(),
            slideshow_plans: Vec::new(),
            removals: Vec::new(),
            errors: Vec::new(),
            decisions: Vec::new(),
        }
    }
}
