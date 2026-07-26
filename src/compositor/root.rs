use std::{cell::RefCell, ffi::OsString, rc::Rc};

mod ipc;
mod launch;
mod signal;

use calloop::channel::{Channel as CalloopChannel, Event as ChannelEvent, sync_channel};
use tensor_runtime::{
    CompletionRelayError, EventfdCompletionRelay, WorkerBridge, WorkerRx, WorkerTx,
};
use thiserror::Error;
use tracing::{error, info, warn};

use ipc::drain_ipc_events;
use launch::drain_launch_outcomes;
use signal::drain_signal_events;

use crate::{
    backend::BackendConfig,
    config::{Config, EnvironmentConfig, StartupCommand},
    ipc::{
        IpcControlEvent, IpcError, IpcEvent, IpcRuntime, IpcServer, MAX_PENDING_IPC_CONTROL_EVENTS,
        MAX_PENDING_IPC_REQUESTS,
    },
    layout::{LayoutEngine, LayoutItem, LayoutState, Rect},
    protocol::{ProtocolError, WaylandRuntime},
    render::{DrmNodeError, DrmNodeId, RendererError, RendererTarget, VulkanRenderer},
    service::{EnvironmentValue, SystemdMode, session_environment},
    signals::{MAX_PENDING_SIGNAL_EVENTS, SignalEvent, SignalRuntime, SignalRuntimeError},
    spawn::{
        LaunchOutcome, LaunchRequest, LaunchWorker, LaunchWorkerError, MAX_PENDING_LAUNCHES,
        ProcessLauncher,
    },
    startup::SessionAutostartPermit,
    xwayland::XWaylandConfig,
};

pub struct Compositor {
    protocol: WaylandRuntime,
    ipc: IpcServer,
    ipc_runtime: Option<IpcRuntime>,
    backend_config: BackendConfig,
    launcher: ProcessLauncher,
    launch_outcome_sender: WorkerTx<LaunchOutcome>,
    launch_outcomes: WorkerRx<LaunchOutcome>,
    ipc_event_sender: WorkerTx<IpcEvent>,
    ipc_events: WorkerRx<IpcEvent>,
    ipc_control_sender: WorkerTx<IpcControlEvent>,
    ipc_control_events: WorkerRx<IpcControlEvent>,
    signal_event_sender: WorkerTx<SignalEvent>,
    signal_events: WorkerRx<SignalEvent>,
    signal_runtime: SignalRuntime,
    completion_notifications: CalloopChannel<()>,
    completion_relay: EventfdCompletionRelay,
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
        let (completion_sender, completion_notifications) = sync_channel(1);
        let completion_relay =
            EventfdCompletionRelay::start("tensor-compositor-completions", move |_| {
                // Transitional adapter only: payloads stay in WorkerBridge.
                // A pending notification already guarantees a compositor turn.
                let _ = completion_sender.try_send(());
            })?;
        let (launch_outcome_sender, launch_outcomes) =
            WorkerBridge::bounded_with_wake(MAX_PENDING_LAUNCHES, completion_relay.wake());
        let (ipc_event_sender, ipc_events) =
            WorkerBridge::bounded_with_wake(MAX_PENDING_IPC_REQUESTS, completion_relay.wake());
        let (ipc_control_sender, ipc_control_events) = WorkerBridge::bounded_with_wake(
            MAX_PENDING_IPC_CONTROL_EVENTS,
            completion_relay.wake(),
        );
        let (signal_event_sender, signal_events) =
            WorkerBridge::bounded_with_wake(MAX_PENDING_SIGNAL_EVENTS, completion_relay.wake());
        let signal_runtime = SignalRuntime::start(signal_event_sender.clone())?;
        Ok(Self {
            protocol,
            ipc,
            ipc_runtime: None,
            backend_config,
            launcher: ProcessLauncher::new(systemd),
            launch_outcome_sender,
            launch_outcomes,
            ipc_event_sender,
            ipc_events,
            ipc_control_sender,
            ipc_control_events,
            signal_event_sender,
            signal_events,
            signal_runtime,
            completion_notifications,
            completion_relay,
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
                Some((index, program.clone(), args.to_vec()))
            })
            .collect::<Vec<_>>();
        if requests.is_empty() {
            return;
        }
        let submitter = match self.ensure_launch_worker() {
            Ok(worker) => worker.submitter(),
            Err(error) => {
                warn!(%error, "could not start the asynchronous launch worker");
                return;
            }
        };
        for (index, program, args) in requests {
            let token = self.protocol.state_mut().issue_spawn_activation_token();
            let request = LaunchRequest::new(
                index as u64,
                program.as_str(),
                args.iter().map(String::as_str),
            )
            .with_activation_token(token);
            match submitter.submit(request) {
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
        self.ensure_ipc_runtime()?;
        Ok(())
    }

    fn ensure_ipc_runtime(&mut self) -> Result<(), IpcError> {
        if self.ipc_runtime.is_none() {
            self.ipc_runtime = Some(self.ipc.start(
                self.ipc_event_sender.clone(),
                self.ipc_control_sender.clone(),
            )?);
        }
        Ok(())
    }

    pub fn run(mut self) -> Result<(), CompositorError> {
        self.ensure_ipc_runtime()?;
        let launch_submitter = self.ensure_launch_worker()?.submitter();
        let Self {
            mut protocol,
            ipc,
            ipc_runtime,
            backend_config: _,
            launcher,
            launch_outcome_sender,
            launch_outcomes,
            ipc_event_sender,
            ipc_events,
            ipc_control_sender,
            ipc_control_events,
            signal_event_sender,
            signal_events,
            signal_runtime,
            completion_notifications,
            completion_relay,
            launch_worker,
            startup_commands,
            environment,
            systemd,
            xwayland,
        } = self;
        let _runtime_owners = (
            ipc_runtime,
            ipc,
            launcher,
            launch_worker,
            launch_outcome_sender,
            ipc_event_sender,
            ipc_control_sender,
            signal_event_sender,
            signal_runtime,
            completion_relay,
            startup_commands,
            environment,
            systemd,
            xwayland,
        );
        let stop_signal = protocol.stop_signal();
        let callback_stop = stop_signal.clone();
        let runtime_failure = Rc::new(RefCell::new(None));
        let callback_failure = Rc::clone(&runtime_failure);
        protocol.run_with_channel(completion_notifications, move |event, state| match event {
            ChannelEvent::Msg(()) => {
                drain_signal_events(&signal_events, &callback_stop, &callback_failure);
                drain_launch_outcomes(&launch_outcomes, state);
                drain_ipc_events(
                    &ipc_events,
                    &ipc_control_events,
                    state,
                    &callback_stop,
                    &launch_submitter,
                    &callback_failure,
                );
            }
            ChannelEvent::Closed => {
                let message = "Compio completion relay disconnected".to_owned();
                error!(%message);
                callback_failure.borrow_mut().replace(message);
                callback_stop.stop();
            }
        })?;
        if let Some(message) = runtime_failure.borrow_mut().take() {
            return Err(CompositorError::CompletionRuntime(message));
        }
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
    #[error(transparent)]
    CompletionRelay(#[from] CompletionRelayError),
    #[error(transparent)]
    SignalRuntime(#[from] SignalRuntimeError),
    #[error("completion service failed: {0}")]
    CompletionRuntime(String),
}

#[cfg(test)]
mod tests {
    use super::ipc::handle_ipc_request;
    use super::*;
    use crate::{
        ecs::{ViewId, WorkspaceId},
        ipc::{Command as IpcCommand, IPC_PROTOCOL_VERSION, Request, ResultBody},
        layout::LayoutKind,
        protocol::RuntimeState,
        scene::SceneAppearance,
    };
    fn runtime_state() -> RuntimeState {
        crate::protocol::test_runtime_state(
            LayoutEngine::new(LayoutKind::Scrolling1D),
            SceneAppearance::default(),
        )
    }

    fn live_worker() -> (LaunchWorker, WorkerRx<LaunchOutcome>) {
        let (outcomes, receiver) = WorkerBridge::bounded(4);
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

        let response = handle_ipc_request(request, &mut state, &submitter);

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
            &submitter,
        );
        assert!(matches!(changed.response.result, ResultBody::Accepted));

        let reply = handle_ipc_request(
            Request::new(13, IpcCommand::GetState),
            &mut state,
            &submitter,
        );
        let ResultBody::State(snapshot) = reply.response.result else {
            panic!("expected IPC state response");
        };
        assert_eq!(snapshot.layout, LayoutKind::Spatial2D);
        assert_eq!(snapshot.view_count, 1);
        assert_eq!(snapshot.output_count, 0);
        assert_eq!(snapshot.focused_view, None);
        assert_eq!(snapshot.workspace, 0);
        assert_eq!(snapshot.workspace_count, 9);
        assert_eq!(state.layout.options(), options);

        let workspaces = handle_ipc_request(
            Request::new(14, IpcCommand::GetWorkspaces),
            &mut state,
            &submitter,
        );
        let ResultBody::Workspaces(list) = workspaces.response.result else {
            panic!("expected workspace list");
        };
        assert_eq!(list.len(), 9);
        assert!(list[0].active);
        assert_eq!(list[0].view_count, 1);
        assert!(!list[1].active);
        assert_eq!(list[1].view_count, 0);

        assert!(matches!(
            handle_ipc_request(
                Request::new(15, IpcCommand::SetWorkspace { index: 2 }),
                &mut state,
                &submitter,
            )
            .response
            .result,
            ResultBody::Accepted
        ));
        assert_eq!(state.active_workspace().get(), 2);
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
            &submitter,
        );
        let ResultBody::Outputs(outputs) = reply.response.result else {
            panic!("expected IPC outputs response");
        };
        assert!(outputs.is_empty());
        drop(worker);
    }

    #[test]
    fn spawn_issues_a_nonempty_activation_token() {
        let mut state = runtime_state();
        let token = state.issue_spawn_activation_token();
        assert!(!token.is_empty());
        assert_eq!(
            LaunchRequest::new(1, "true", std::iter::empty::<&str>())
                .with_activation_token(token.clone())
                .activation_token()
                .map(|value| value.to_string_lossy().into_owned()),
            Some(token)
        );
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
            &submitter,
        );
        assert!(matches!(response.response.result, ResultBody::Accepted));

        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let outcome = loop {
            if let Some(outcome) = receiver.try_recv() {
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
