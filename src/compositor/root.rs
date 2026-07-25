use std::ffi::OsString;

use smithay::reexports::calloop::channel::{
    Channel as CalloopChannel, Event as ChannelEvent, SyncSender as CalloopSender, sync_channel,
};
use thiserror::Error;
use tracing::{info, warn};

use crate::{
    backend::BackendConfig,
    config::{Config, EnvironmentConfig, StartupCommand},
    ipc::{
        Command as IpcCommand, IPC_PROTOCOL_VERSION, IpcError, IpcReply, IpcServer, Request,
        Response, ResultBody,
    },
    layout::{LayoutEngine, LayoutItem, LayoutState, Rect},
    protocol::{ProtocolError, RuntimeState, WaylandRuntime},
    render::{DrmNodeError, DrmNodeId, RendererError, RendererTarget, VulkanRenderer},
    service::{EnvironmentValue, SystemdMode, session_environment},
    spawn::{
        LaunchOutcome, LaunchRequest, LaunchSubmitError, LaunchSubmitter, LaunchWorker,
        LaunchWorkerError, MAX_PENDING_LAUNCHES, ProcessLauncher,
    },
    startup::SessionAutostartPermit,
    xwayland::XWaylandConfig,
};

pub struct Compositor {
    protocol: WaylandRuntime,
    ipc: IpcServer,
    backend_config: BackendConfig,
    launcher: ProcessLauncher,
    launch_outcome_sender: CalloopSender<LaunchOutcome>,
    launch_outcomes: CalloopChannel<LaunchOutcome>,
    launch_worker: Option<LaunchWorker>,
    startup_commands: Vec<StartupCommand>,
    environment: EnvironmentConfig,
    systemd: SystemdMode,
    xwayland: XWaylandConfig,
}

impl Compositor {
    pub fn new(config: Config) -> Result<Self, CompositorError> {
        let Config {
            initial_layout,
            layout_options,
            ipc_socket,
            gpu_preference,
            render_device,
            output_rules,
            appearance,
            systemd,
            xwayland,
            startup_commands,
            environment,
            cursor,
            debug,
        } = config;
        let mut protocol = WaylandRuntime::with_appearance(
            LayoutEngine::with_options(initial_layout, layout_options),
            appearance,
        )?;
        protocol.state_mut().apply_runtime_policy(cursor, debug);
        let requested_drm_node = render_device
            .as_deref()
            .map(DrmNodeId::from_path)
            .transpose()?;
        let renderer = VulkanRenderer::new(RendererTarget::with_device(
            gpu_preference,
            requested_drm_node,
        ))?;
        let backend_config = BackendConfig {
            drm_node: renderer.selected().render_node,
            renderer_formats: renderer.selected().formats.clone(),
            output_rules,
        };
        protocol.install_renderer(renderer);
        let ipc = IpcServer::bind(ipc_socket)?;
        let (launch_outcome_sender, launch_outcomes) = sync_channel(MAX_PENDING_LAUNCHES);
        Ok(Self {
            protocol,
            ipc,
            backend_config,
            launcher: ProcessLauncher::new(systemd),
            launch_outcome_sender,
            launch_outcomes,
            launch_worker: None,
            startup_commands,
            environment,
            systemd,
            xwayland,
        })
    }

    pub fn check_ready(&mut self) {
        let Some(renderer) = self.protocol.renderer() else {
            return;
        };
        let renderer_target = renderer.target();
        let selected_device = renderer.selected().clone();
        let renderer_outputs = renderer.output_count();
        let (
            preview_views,
            layout,
            layout_options,
            ecs_views,
            outputs,
            seat,
            xdg_output,
            protocol_globals,
        ) = {
            let state = self.protocol.state_mut();
            let mut preview_state = LayoutState::default();
            let preview_items = [LayoutItem::default(); 3];
            (
                state
                    .layout
                    .arrange(
                        &mut preview_state,
                        Rect::new(0, 0, 1920, 1080),
                        &preview_items,
                        Some(0),
                    )
                    .placements
                    .len(),
                state.layout.kind(),
                state.layout.options(),
                state.view_count(),
                state.output_count(),
                state.seat.name().to_owned(),
                state
                    .output_manager_state
                    .xdg_output_manager_global()
                    .is_some(),
                state.protocol_globals.capabilities(),
            )
        };
        info!(
            protocol = self.protocol.backend_name(),
            wayland_socket = ?self.protocol.socket_name(),
            ipc = %self.ipc.path().display(),
            renderer_outputs,
            vulkan = %renderer_target.api_version,
            descriptors = renderer_target.descriptor_heap.name(),
            gpu_preference = renderer_target.device.preference().name(),
            gpu = selected_device.name,
            gpu_type = ?selected_device.device_type,
            graphics_queue_family = selected_device.graphics_queue_family,
            layout = layout.name(),
            layout_options = ?layout_options,
            systemd = self.systemd.name(),
            spawn_strategy = self.launcher.strategy().name(),
            startup_commands = self.startup_commands.len(),
            xwayland = self.xwayland.enabled(),
            preview_views,
            ecs_views,
            outputs,
            seat,
            xdg_output,
            protocol_globals = ?protocol_globals,
            "compositor runtime is ready"
        );
    }

    pub(crate) fn spawn_startup_commands(&mut self, _permit: SessionAutostartPermit) {
        let requests = self
            .startup_commands
            .iter()
            .enumerate()
            .filter_map(|(index, command)| {
                let (program, args) = command.argv.split_first()?;
                Some((
                    index,
                    program.clone(),
                    LaunchRequest::new(
                        index as u64,
                        program.as_str(),
                        args.iter().map(String::as_str),
                    ),
                ))
            })
            .collect::<Vec<_>>();
        if requests.is_empty() {
            return;
        }
        let worker = match self.ensure_launch_worker() {
            Ok(worker) => worker,
            Err(error) => {
                warn!(%error, "could not start the asynchronous launch worker");
                return;
            }
        };
        for (index, program, request) in requests {
            match worker.submit(request) {
                Ok(()) => info!(request_id = index, program, "startup command queued"),
                Err(error) => {
                    warn!(request_id = index, program, %error, "startup command rejected")
                }
            }
        }
    }

    fn ensure_launch_worker(&mut self) -> Result<&LaunchWorker, LaunchWorkerError> {
        if self.launch_worker.is_none() {
            self.launch_worker = Some(LaunchWorker::new(
                self.launcher.clone(),
                self.launch_outcome_sender.clone(),
            )?);
        }
        Ok(self
            .launch_worker
            .as_ref()
            .expect("launch worker was installed"))
    }

    pub(crate) fn publish_session_environment(
        &mut self,
    ) -> Result<Vec<EnvironmentValue>, CompositorError> {
        let mut environment = session_environment(
            self.protocol.socket_name().to_os_string(),
            OsString::from(self.ipc.path()),
            self.protocol.xwayland_display(),
        );
        apply_user_environment(&mut environment, &self.environment);
        self.launcher
            .set_environment_clear(self.environment.clear.iter().cloned());
        self.launcher.set_environment(environment.clone());
        Ok(environment)
    }

    pub fn prepare_runtime(&mut self) -> Result<(), CompositorError> {
        self.protocol.prepare(self.xwayland.enabled())?;
        self.protocol.prepare_backend(&self.backend_config)?;
        Ok(())
    }

    pub fn run(mut self) -> Result<(), CompositorError> {
        // IPC spawn and optional late autostart share one worker. Create it
        // before calloop takes ownership so the submit handle can be cloned
        // into the IPC callback without holding Smithay objects.
        let launch_submitter = self.ensure_launch_worker()?.submitter();
        let Self {
            mut protocol,
            ipc,
            backend_config: _,
            launcher,
            launch_outcome_sender,
            launch_outcomes,
            launch_worker,
            startup_commands,
            environment,
            systemd,
            xwayland,
        } = self;
        // Keep non-loop owners alive for the whole run without naming them in
        // the IPC closures. launch_outcomes moves into calloop; the worker and
        // outcome sender stay here until the loop returns.
        let _runtime_owners = (
            launcher,
            launch_outcome_sender,
            launch_worker,
            startup_commands,
            environment,
            systemd,
            xwayland,
        );
        let stop_signal = protocol.stop_signal();
        protocol.run_with_ipc_and_channel(
            &ipc,
            launch_outcomes,
            handle_launch_outcome,
            move |request, state| {
                let reflow = matches!(&request.command, IpcCommand::SetLayout { .. });
                let reply = handle_ipc_request(request, state, &stop_signal, &launch_submitter);
                if reflow {
                    state.reflow_default_workspace();
                }
                reply
            },
        )?;
        Ok(())
    }
}

/// Merge user `[environment]` policy into the session publication snapshot.
///
/// Session-owned names (`WAYLAND_DISPLAY`, …) always win over user `set` values
/// so a misconfigured file cannot break the compositor boundary. `clear` only
/// removes non-session keys that a later `set` might reintroduce; session keys
/// are never cleared here because they were just written by `session_environment`.
fn apply_user_environment(environment: &mut Vec<EnvironmentValue>, policy: &EnvironmentConfig) {
    use std::collections::BTreeSet;

    let session_names: BTreeSet<OsString> = crate::service::SESSION_ENVIRONMENT_NAMES
        .iter()
        .map(|name| OsString::from(*name))
        .collect();
    for name in &policy.clear {
        let key = OsString::from(name);
        if session_names.contains(&key) {
            continue;
        }
        environment.retain(|(existing, _)| existing != &key);
    }
    for (name, value) in &policy.set {
        let key = OsString::from(name);
        if session_names.contains(&key) {
            warn!(
                name,
                "ignoring [environment].set for a session-owned variable"
            );
            continue;
        }
        if let Some(entry) = environment
            .iter_mut()
            .find(|(existing, _)| existing == &key)
        {
            entry.1 = OsString::from(value);
        } else {
            environment.push((key, OsString::from(value)));
        }
    }
}

fn handle_launch_outcome(event: ChannelEvent<LaunchOutcome>, _: &mut RuntimeState) {
    match event {
        ChannelEvent::Msg(outcome) => match outcome.result() {
            Ok(process) => info!(
                request_id = outcome.id(),
                program = ?outcome.program(),
                pid = process.pid(),
                strategy = process.strategy().name(),
                "application launch completed"
            ),
            Err(error) => warn!(
                request_id = outcome.id(),
                program = ?outcome.program(),
                %error,
                "application launch failed"
            ),
        },
        ChannelEvent::Closed => warn!("asynchronous launch worker disconnected"),
    }
}

fn handle_ipc_request(
    request: Request,
    state: &mut RuntimeState,
    stop_signal: &smithay::reexports::calloop::LoopSignal,
    launch_submitter: &LaunchSubmitter,
) -> IpcReply {
    let request_id = request.request_id;
    if request.version != IPC_PROTOCOL_VERSION {
        return IpcReply::new(Response::error(
            request_id,
            "unsupported_version",
            format!(
                "protocol version {} is unsupported; expected {IPC_PROTOCOL_VERSION}",
                request.version
            ),
        ));
    }

    let result = match request.command {
        IpcCommand::Ping => ResultBody::Pong,
        IpcCommand::GetState => ResultBody::State(state.ipc_state_snapshot()),
        IpcCommand::GetOutputs => ResultBody::Outputs(state.ipc_output_snapshots()),
        IpcCommand::SetLayout { layout: kind } => {
            state.layout = LayoutEngine::with_options(kind, state.layout.options());
            state.world.reset_layout_states();
            ResultBody::Accepted
        }
        IpcCommand::Spawn { argv } => match queue_spawn(request_id, argv, launch_submitter) {
            Ok(()) => ResultBody::Accepted,
            Err((code, message)) => {
                return IpcReply::new(Response::error(request_id, code, message));
            }
        },
        IpcCommand::Quit => {
            return IpcReply::stop_after_flush(
                Response::new(request_id, ResultBody::Accepted),
                stop_signal.clone(),
            );
        }
    };
    IpcReply::new(Response::new(request_id, result))
}

fn queue_spawn(
    request_id: u64,
    argv: Vec<String>,
    launch_submitter: &LaunchSubmitter,
) -> Result<(), (&'static str, String)> {
    let Some((program, args)) = argv.split_first() else {
        return Err((
            "invalid_argument",
            "spawn requires a non-empty argv".to_owned(),
        ));
    };
    if program.is_empty() {
        return Err((
            "invalid_argument",
            "spawn program must not be empty".to_owned(),
        ));
    }
    let request = LaunchRequest::new(
        request_id,
        program.as_str(),
        args.iter().map(String::as_str),
    );
    match launch_submitter.submit(request) {
        Ok(()) => {
            info!(request_id, program, "IPC spawn queued");
            Ok(())
        }
        Err(LaunchSubmitError::QueueFull { id }) => Err((
            "queue_full",
            format!("launch queue is full for request {id}"),
        )),
        Err(LaunchSubmitError::WorkerStopped { id }) => Err((
            "worker_stopped",
            format!("launch worker stopped before accepting request {id}"),
        )),
    }
}

#[derive(Debug, Error)]
pub enum CompositorError {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error(transparent)]
    Ipc(#[from] IpcError),
    #[error(transparent)]
    Renderer(#[from] RendererError),
    #[error(transparent)]
    DrmNode(#[from] DrmNodeError),
    #[error(transparent)]
    LaunchWorker(#[from] LaunchWorkerError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ecs::{ViewId, WorkspaceId},
        layout::LayoutKind,
        scene::SceneAppearance,
    };
    use smithay::reexports::{calloop::EventLoop, wayland_server::Display};

    fn stop_signal() -> smithay::reexports::calloop::LoopSignal {
        EventLoop::<()>::try_new().unwrap().get_signal()
    }

    fn runtime_state() -> RuntimeState {
        let display = Display::<RuntimeState>::new().unwrap();
        RuntimeState::with_appearance(
            display.handle(),
            EventLoop::<RuntimeState>::try_new().unwrap().handle(),
            LayoutEngine::new(LayoutKind::Scrolling1D),
            SceneAppearance::default(),
        )
    }

    fn live_worker() -> (
        LaunchWorker,
        smithay::reexports::calloop::channel::Channel<LaunchOutcome>,
    ) {
        let (outcomes, receiver) =
            smithay::reexports::calloop::channel::sync_channel::<LaunchOutcome>(4);
        let worker = LaunchWorker::new(
            ProcessLauncher::with_systemd_detection(SystemdMode::Disabled, false),
            outcomes,
        )
        .unwrap();
        (worker, receiver)
    }

    #[test]
    fn ipc_rejects_unknown_protocol_versions() {
        let mut state = runtime_state();
        let mut request = Request::new(11, IpcCommand::Ping);
        request.version = IPC_PROTOCOL_VERSION + 1;
        let (worker, _) = live_worker();
        let submitter = worker.submitter();

        let response = handle_ipc_request(request, &mut state, &stop_signal(), &submitter);

        assert_eq!(response.response.request_id, 11);
        assert!(matches!(response.response.result, ResultBody::Error(_)));
        drop(worker);
    }

    #[test]
    fn ipc_layout_change_is_visible_in_state() {
        let mut state = runtime_state();
        state
            .world
            .spawn_view(ViewId::new(1), WorkspaceId::new(0))
            .unwrap();
        let options = crate::layout::LayoutOptions {
            gap: 17,
            ..Default::default()
        };
        state.layout = LayoutEngine::with_options(LayoutKind::Scrolling1D, options);
        let (worker, _) = live_worker();
        let submitter = worker.submitter();

        let changed = handle_ipc_request(
            Request::new(
                12,
                IpcCommand::SetLayout {
                    layout: LayoutKind::Spatial2D,
                },
            ),
            &mut state,
            &stop_signal(),
            &submitter,
        );
        assert!(matches!(changed.response.result, ResultBody::Accepted));

        let reply = handle_ipc_request(
            Request::new(13, IpcCommand::GetState),
            &mut state,
            &stop_signal(),
            &submitter,
        );
        let ResultBody::State(snapshot) = reply.response.result else {
            panic!("expected IPC state response");
        };
        assert_eq!(snapshot.layout, LayoutKind::Spatial2D);
        assert_eq!(snapshot.view_count, 1);
        assert_eq!(snapshot.output_count, 0);
        assert_eq!(snapshot.focused_view, None);
        assert_eq!(state.layout.options(), options);
        drop(worker);
    }

    #[test]
    fn ipc_get_outputs_returns_empty_without_heads() {
        let mut state = runtime_state();
        let (worker, _) = live_worker();
        let submitter = worker.submitter();

        let reply = handle_ipc_request(
            Request::new(16, IpcCommand::GetOutputs),
            &mut state,
            &stop_signal(),
            &submitter,
        );
        let ResultBody::Outputs(outputs) = reply.response.result else {
            panic!("expected IPC outputs response");
        };
        assert!(outputs.is_empty());
        drop(worker);
    }

    #[test]
    fn user_environment_cannot_override_session_names() {
        let mut environment = session_environment("wayland-1", "/tmp/tensor.sock", None);
        apply_user_environment(
            &mut environment,
            &EnvironmentConfig {
                clear: vec!["WAYLAND_DISPLAY".to_owned(), "EDITOR".to_owned()],
                set: [
                    ("WAYLAND_DISPLAY".to_owned(), "evil".to_owned()),
                    ("EDITOR".to_owned(), "hx".to_owned()),
                ]
                .into_iter()
                .collect(),
            },
        );
        assert_eq!(
            environment
                .iter()
                .find(|(name, _)| name == "WAYLAND_DISPLAY")
                .map(|(_, value)| value.as_os_str()),
            Some(std::ffi::OsStr::new("wayland-1"))
        );
        assert_eq!(
            environment
                .iter()
                .find(|(name, _)| name == "EDITOR")
                .map(|(_, value)| value.as_os_str()),
            Some(std::ffi::OsStr::new("hx"))
        );
    }

    #[test]
    fn ipc_spawn_rejects_empty_argv() {
        let mut state = runtime_state();
        let (worker, _) = live_worker();
        let submitter = worker.submitter();

        let response = handle_ipc_request(
            Request::new(14, IpcCommand::Spawn { argv: Vec::new() }),
            &mut state,
            &stop_signal(),
            &submitter,
        );

        let ResultBody::Error(error) = response.response.result else {
            panic!("expected spawn rejection");
        };
        assert_eq!(error.code, "invalid_argument");
        drop(worker);
    }

    #[test]
    fn ipc_spawn_queues_on_a_live_worker() {
        use std::time::Duration;

        let mut state = runtime_state();
        let (worker, receiver) = live_worker();
        let submitter = worker.submitter();
        let program = format!("tensor-missing-ipc-spawn-{}", std::process::id());

        let response = handle_ipc_request(
            Request::new(
                15,
                IpcCommand::Spawn {
                    argv: vec![program.clone()],
                },
            ),
            &mut state,
            &stop_signal(),
            &submitter,
        );
        assert!(matches!(response.response.result, ResultBody::Accepted));

        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let outcome = loop {
            if let Ok(outcome) = receiver.try_recv() {
                break outcome;
            }
            if std::time::Instant::now() >= deadline {
                panic!("launch worker should report the missing program");
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(outcome.id(), 15);
        assert_eq!(outcome.program(), std::ffi::OsStr::new(&program));
        assert!(outcome.result().is_err());
        drop(worker);
    }
}
