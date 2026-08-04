use std::{cell::RefCell, ffi::OsString, path::PathBuf, rc::Rc, sync::Arc};

mod config;
mod environment;
#[cfg(feature = "tty")]
mod gpu;
mod ipc;
mod launch;
mod signal;
mod wayland_completion;
#[cfg(test)]
mod workspace_tests;

use tensor_runtime::{
    EventfdWake, EventfdWakeError, RuntimeStop, WakeSink, WorkerBridge, WorkerRx, WorkerTx,
};
use thiserror::Error;
use tracing::{error, info, warn};

use config::drain_config_reload_outcomes;
use environment::apply_user_environment;
#[cfg(feature = "tty")]
use gpu::drain_gpu_fence_events;
use ipc::{ConfigIpcContext, drain_ipc_events};
use launch::drain_launch_outcomes;
use signal::drain_signal_events;
use wayland_completion::WaylandCompletionBridges;

#[cfg(feature = "tty")]
use crate::render::{GpuFenceEvent, GpuFenceRuntime, GpuFenceRuntimeError, MAX_PENDING_GPU_FENCES};
use crate::{
    backend::BackendConfig,
    config::{
        Config, ConfigReloadOutcome, ConfigReloadWorker, ConfigReloadWorkerError,
        ConfigTransaction, ConfigWatcher, CursorConfig, EnvironmentConfig,
        MAX_PENDING_CONFIG_RELOAD_RESULTS, StartupCommand,
    },
    ipc::{
        IpcControlEvent, IpcError, IpcEvent, IpcRuntime, IpcServer, MAX_PENDING_IPC_CONTROL_EVENTS,
        MAX_PENDING_IPC_REQUESTS,
    },
    layout::{LayoutEngine, LayoutItem, LayoutState, Rect},
    protocol::{
        MAX_PENDING_SECURITY_CONTEXT_EVENTS, ProtocolError, SecurityContextEvent,
        SecurityContextRuntime, SecurityContextRuntimeError, WaylandRuntime,
        drain_security_context_events,
    },
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
    security_context_events: WorkerRx<SecurityContextEvent>,
    security_context_runtime: SecurityContextRuntime,
    wayland_completions: WaylandCompletionBridges,
    #[cfg(feature = "tty")]
    gpu_fence_event_sender: WorkerTx<GpuFenceEvent>,
    #[cfg(feature = "tty")]
    gpu_fence_events: WorkerRx<GpuFenceEvent>,
    #[cfg(feature = "tty")]
    gpu_fence_runtime: GpuFenceRuntime,
    completion_wake: Arc<EventfdWake>,
    config_transaction: ConfigTransaction,
    config_reload_sender: WorkerTx<ConfigReloadOutcome>,
    config_reload_outcomes: WorkerRx<ConfigReloadOutcome>,
    config_reload_worker: Option<ConfigReloadWorker>,
    config_watcher: Option<ConfigWatcher>,
    launch_worker: Option<LaunchWorker>,
    startup_commands: Vec<StartupCommand>,
    environment: EnvironmentConfig,
    cursor: CursorConfig,
    systemd: SystemdMode,
    xwayland: XWaylandConfig,
}

impl Compositor {
    pub fn new(config_path: PathBuf, config: Config) -> Result<Self, CompositorError> {
        let config_transaction = ConfigTransaction::new(config_path, config.clone());
        let Config {
            initial_layout,
            layout_options,
            workspaces,
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
        protocol.state_mut().configure_workspaces(&workspaces);
        protocol
            .state_mut()
            .apply_runtime_policy(cursor.clone(), debug);
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
        let completion_wake = Arc::new(EventfdWake::new()?);
        let completion_sink = Arc::clone(&completion_wake) as Arc<dyn WakeSink>;
        let wayland_completions =
            WaylandCompletionBridges::install(&mut protocol, Arc::clone(&completion_sink))?;
        let (launch_outcome_sender, launch_outcomes) =
            WorkerBridge::bounded_with_wake(MAX_PENDING_LAUNCHES, Arc::clone(&completion_sink));
        let (ipc_event_sender, ipc_events) =
            WorkerBridge::bounded_with_wake(MAX_PENDING_IPC_REQUESTS, Arc::clone(&completion_sink));
        let (ipc_control_sender, ipc_control_events) = WorkerBridge::bounded_with_wake(
            MAX_PENDING_IPC_CONTROL_EVENTS,
            Arc::clone(&completion_sink),
        );
        let (signal_event_sender, signal_events) = WorkerBridge::bounded_with_wake(
            MAX_PENDING_SIGNAL_EVENTS,
            Arc::clone(&completion_sink),
        );
        let (config_reload_sender, config_reload_outcomes) = WorkerBridge::bounded_with_wake(
            MAX_PENDING_CONFIG_RELOAD_RESULTS,
            Arc::clone(&completion_sink),
        );
        let signal_runtime = SignalRuntime::start(signal_event_sender.clone())?;
        let (security_context_sender, security_context_events) = WorkerBridge::bounded_with_wake(
            MAX_PENDING_SECURITY_CONTEXT_EVENTS,
            Arc::clone(&completion_sink),
        );
        let security_context_runtime = SecurityContextRuntime::start(security_context_sender)?;
        protocol
            .state_mut()
            .install_security_context_submitter(security_context_runtime.submitter());
        #[cfg(feature = "tty")]
        let (gpu_fence_event_sender, gpu_fence_events) =
            WorkerBridge::bounded_with_wake(MAX_PENDING_GPU_FENCES, completion_sink);
        #[cfg(feature = "tty")]
        let gpu_fence_runtime = GpuFenceRuntime::start(gpu_fence_event_sender.clone())?;
        #[cfg(feature = "tty")]
        protocol
            .state_mut()
            .install_gpu_fence_submitter(gpu_fence_runtime.submitter());
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
            security_context_events,
            security_context_runtime,
            wayland_completions,
            #[cfg(feature = "tty")]
            gpu_fence_event_sender,
            #[cfg(feature = "tty")]
            gpu_fence_events,
            #[cfg(feature = "tty")]
            gpu_fence_runtime,
            completion_wake,
            config_transaction,
            config_reload_sender,
            config_reload_outcomes,
            config_reload_worker: None,
            config_watcher: None,
            launch_worker: None,
            startup_commands,
            environment,
            cursor,
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
                "tensorland".to_owned(),
                state.protocol_globals.xdg_output_enabled(),
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
            let token = match self.protocol.state_mut().issue_spawn_activation_token() {
                Ok(token) => token,
                Err(error) => {
                    warn!(request_id = index, program, %error, "could not issue launch activation token");
                    continue;
                }
            };
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

    fn ensure_config_reload_worker(
        &mut self,
    ) -> Result<&ConfigReloadWorker, ConfigReloadWorkerError> {
        if self.config_reload_worker.is_none() {
            self.config_reload_worker = Some(ConfigReloadWorker::start(
                self.config_transaction.path().to_owned(),
                self.config_reload_sender.clone(),
            )?);
        }
        Ok(self
            .config_reload_worker
            .as_ref()
            .expect("configuration reload worker was installed"))
    }

    fn ensure_config_runtime(&mut self) -> Result<(), CompositorError> {
        let reload = self
            .ensure_config_reload_worker()
            .map_err(|error| CompositorError::ConfigReloadWorker(error.to_string()))?
            .submitter();
        if self.config_watcher.is_none() {
            self.config_watcher = Some(
                ConfigWatcher::start(self.config_transaction.path().to_owned(), reload)
                    .map_err(|error| CompositorError::ConfigWatcher(error.to_string()))?,
            );
        }
        Ok(())
    }

    pub(crate) fn publish_session_environment(
        &mut self,
    ) -> Result<Vec<EnvironmentValue>, CompositorError> {
        let mut environment = session_environment(
            self.protocol.socket_name().to_os_string(),
            OsString::from(self.ipc.path()),
            self.protocol.xwayland_display(),
            self.cursor.theme.clone(),
            self.cursor.size,
        );
        apply_user_environment(&mut environment, &self.environment);
        self.launcher
            .set_environment_clear(self.environment.clear.iter().cloned());
        self.launcher.set_environment(environment.clone());
        Ok(environment)
    }

    pub fn prepare_runtime(&mut self) -> Result<(), CompositorError> {
        self.protocol.prepare(self.xwayland.enabled())?;
        self.protocol.prepare_backend(
            &self.backend_config,
            Arc::clone(&self.completion_wake) as Arc<dyn WakeSink>,
        )?;
        self.ensure_ipc_runtime()?;
        self.ensure_config_runtime()?;
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
        self.ensure_config_runtime()?;
        let config_reload_submitter = self
            .ensure_config_reload_worker()
            .map_err(|error| CompositorError::ConfigReloadWorker(error.to_string()))?
            .submitter();
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
            security_context_events,
            security_context_runtime,
            wayland_completions,
            #[cfg(feature = "tty")]
            gpu_fence_event_sender,
            #[cfg(feature = "tty")]
            gpu_fence_events,
            #[cfg(feature = "tty")]
            gpu_fence_runtime,
            completion_wake,
            mut config_transaction,
            config_reload_sender,
            config_reload_outcomes,
            config_reload_worker,
            config_watcher,
            launch_worker,
            startup_commands,
            environment,
            cursor: _,
            systemd,
            xwayland,
        } = self;
        let wayland_socket_runtime = protocol.take_socket_runtime();
        let wayland_display_runtime = protocol.take_display_runtime();
        #[cfg(feature = "xwayland")]
        let xwayland_completion_runtime = protocol.take_xwayland_completion_runtime();
        let _runtime_owners = (
            wayland_socket_runtime,
            wayland_display_runtime,
            ipc_runtime,
            ipc,
            launcher,
            launch_worker,
            launch_outcome_sender,
            ipc_event_sender,
            ipc_control_sender,
            signal_event_sender,
            signal_runtime,
            security_context_runtime,
            config_reload_sender,
            config_reload_worker,
            config_watcher,
            startup_commands,
            environment,
            systemd,
            xwayland,
        );
        #[cfg(feature = "xwayland")]
        let _xwayland_completion_runtime_owner = xwayland_completion_runtime;
        #[cfg(feature = "tty")]
        let _gpu_fence_runtime_owners = (gpu_fence_event_sender, gpu_fence_runtime);
        let stop_signal = RuntimeStop::default();
        let callback_stop = stop_signal.clone();
        let runtime_failure = Rc::new(RefCell::new(None));
        let callback_failure = Rc::clone(&runtime_failure);
        protocol.run_with_completions(&completion_wake, &stop_signal, move |state| {
            if let Err(message) = wayland_completions.drain(state) {
                error!(%message, "Wayland completion runtime failed");
                callback_failure.borrow_mut().replace(message);
                callback_stop.stop();
                return;
            }
            drain_signal_events(&signal_events, &callback_stop, &callback_failure);
            if let Err(message) = drain_security_context_events(&security_context_events, state) {
                error!(%message, "security-context completion runtime failed");
                callback_failure.borrow_mut().replace(message);
                callback_stop.stop();
                return;
            }
            #[cfg(feature = "tty")]
            if let Err(message) = state.drain_backend_completions() {
                error!(%message, "tty completion runtime failed");
                callback_failure.borrow_mut().replace(message);
                callback_stop.stop();
                return;
            }
            #[cfg(feature = "tty")]
            drain_gpu_fence_events(&gpu_fence_events, state, &callback_stop, &callback_failure);
            drain_launch_outcomes(&launch_outcomes, state);
            drain_config_reload_outcomes(&config_reload_outcomes, &mut config_transaction, state);
            drain_ipc_events(
                &ipc_events,
                &ipc_control_events,
                state,
                &callback_stop,
                &launch_submitter,
                ConfigIpcContext {
                    transaction: &config_transaction,
                    reload: &config_reload_submitter,
                },
                &callback_failure,
            );
        })?;
        if let Some(message) = runtime_failure.borrow_mut().take() {
            return Err(CompositorError::CompletionRuntime(message));
        }
        Ok(())
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
    #[error("configuration reload worker failed: {0}")]
    ConfigReloadWorker(String),
    #[error("configuration watcher failed: {0}")]
    ConfigWatcher(String),
    #[error(transparent)]
    CompletionWake(#[from] EventfdWakeError),
    #[error(transparent)]
    SignalRuntime(#[from] SignalRuntimeError),
    #[error(transparent)]
    SecurityContextRuntime(#[from] SecurityContextRuntimeError),
    #[cfg(feature = "tty")]
    #[error(transparent)]
    GpuFenceRuntime(#[from] GpuFenceRuntimeError),
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
        let workspace_config = crate::config::WorkspaceConfig {
            regular_count: 3,
            ..Default::default()
        };
        state.configure_workspaces(&workspace_config);
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
        assert_eq!(snapshot.workspace_count, 3);
        assert_eq!(snapshot.hidden_workspace_count, 1);
        assert_eq!(snapshot.minimized_count, 0);
        assert_eq!(state.layout.options(), options);

        let workspaces = handle_ipc_request(
            Request::new(14, IpcCommand::GetWorkspaces),
            &mut state,
            &submitter,
        );
        let ResultBody::Workspaces(list) = workspaces.response.result else {
            panic!("expected workspace list");
        };
        assert_eq!(list.len(), 4);
        assert!(list[0].active);
        assert_eq!(list[0].view_count, 1);
        assert!(!list[1].active);
        assert_eq!(list[1].view_count, 0);
        assert!(list[3].hidden);
        assert!(list[3].minimize_target);
        assert_eq!(list[3].name, "minimized");

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
        let token = state.issue_spawn_activation_token().unwrap();
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
