//! Compositor-side handling for values produced by completed IPC operations.

use std::{cell::RefCell, rc::Rc};

use tensor_runtime::{RuntimeStop, WorkerRx};
use tracing::{error, info};

use crate::{
    config::{ConfigReloadSubmitError, ConfigReloadSubmitter, ConfigTransaction},
    ipc::{
        Command as IpcCommand, ConfigStatusSnapshot, IPC_PROTOCOL_VERSION, IpcControlEvent,
        IpcEvent, IpcReply, MAX_PENDING_IPC_CONTROL_EVENTS, MAX_PENDING_IPC_REQUESTS, Request,
        Response, ResultBody,
    },
    layout::LayoutEngine,
    protocol::RuntimeState,
    spawn::{LaunchRequest, LaunchSubmitError, LaunchSubmitter},
};

pub(super) fn drain_ipc_events(
    requests: &WorkerRx<IpcEvent>,
    control: &WorkerRx<IpcControlEvent>,
    state: &mut RuntimeState,
    stop_signal: &RuntimeStop,
    launch_submitter: &LaunchSubmitter,
    config: ConfigIpcContext<'_>,
    runtime_failure: &Rc<RefCell<Option<String>>>,
) {
    requests.drain(
        MAX_PENDING_IPC_REQUESTS,
        |IpcEvent {
             request,
             respond_to,
         }| {
            let reflow = matches!(&request.command, crate::ipc::Command::SetLayout { .. });
            let reply =
                handle_ipc_request_with_config(request, state, launch_submitter, Some(config));
            if reflow {
                state.reflow_default_workspace();
            }
            let _ = respond_to.send(reply);
        },
    );
    control.drain(MAX_PENDING_IPC_CONTROL_EVENTS, |event| match event {
        IpcControlEvent::ShutdownFlushed => stop_signal.stop(),
        IpcControlEvent::RuntimeFailed(message) => {
            error!(%message, "IPC completion runtime failed");
            runtime_failure
                .borrow_mut()
                .replace(format!("IPC: {message}"));
            stop_signal.stop();
        }
    });
}

#[cfg(test)]
pub(super) fn handle_ipc_request(
    request: Request,
    state: &mut RuntimeState,
    launch_submitter: &LaunchSubmitter,
) -> IpcReply {
    handle_ipc_request_with_config(request, state, launch_submitter, None)
}

#[derive(Clone, Copy)]
pub(super) struct ConfigIpcContext<'a> {
    pub(super) transaction: &'a ConfigTransaction,
    pub(super) reload: &'a ConfigReloadSubmitter,
}

pub(super) fn handle_ipc_request_with_config(
    request: Request,
    state: &mut RuntimeState,
    launch_submitter: &LaunchSubmitter,
    config: Option<ConfigIpcContext<'_>>,
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
        IpcCommand::GetWorkspaces => ResultBody::Workspaces(state.ipc_workspace_snapshots()),
        IpcCommand::GetConfigStatus => {
            let Some(config) = config else {
                return IpcReply::new(Response::error(
                    request_id,
                    "service_unavailable",
                    "configuration reload runtime is not installed",
                ));
            };
            ResultBody::ConfigStatus(ConfigStatusSnapshot {
                generation: config.transaction.generation(),
                last_failure: config.transaction.last_failure().cloned(),
            })
        }
        IpcCommand::ReloadConfig => {
            let Some(config) = config else {
                return IpcReply::new(Response::error(
                    request_id,
                    "service_unavailable",
                    "configuration reload runtime is not installed",
                ));
            };
            match config.reload.submit(request_id) {
                Ok(()) => ResultBody::Accepted,
                Err(ConfigReloadSubmitError::QueueFull) => {
                    return IpcReply::new(Response::error(
                        request_id,
                        "queue_full",
                        "configuration reload queue is full",
                    ));
                }
                Err(ConfigReloadSubmitError::WorkerStopped) => {
                    return IpcReply::new(Response::error(
                        request_id,
                        "worker_stopped",
                        "configuration reload worker has stopped",
                    ));
                }
            }
        }
        IpcCommand::SetLayout { layout: kind } => {
            state.layout = LayoutEngine::with_options(kind, state.layout.options());
            state.world.reset_layout_states();
            ResultBody::Accepted
        }
        IpcCommand::Spawn { argv } => {
            match queue_spawn(request_id, argv, launch_submitter, state) {
                Ok(()) => ResultBody::Accepted,
                Err((code, message)) => {
                    return IpcReply::new(Response::error(request_id, code, message));
                }
            }
        }
        IpcCommand::SetWorkspace { index } => {
            if state.activate_workspace_index(index) {
                ResultBody::Accepted
            } else if index >= state.workspace_count() {
                return IpcReply::new(Response::error(
                    request_id,
                    "invalid_argument",
                    format!(
                        "workspace index {index} out of range (0..{})",
                        state.workspace_count()
                    ),
                ));
            } else {
                ResultBody::Accepted
            }
        }
        IpcCommand::SetOutputPosition { name, x, y } => {
            return apply_output_ipc(request_id, state, name, |rule| {
                rule.position = Some((x, y));
                rule.enabled = true;
            });
        }
        IpcCommand::SetOutputEnabled { name, enabled } => {
            return apply_output_ipc(request_id, state, name, |rule| {
                rule.enabled = enabled;
            });
        }
        IpcCommand::SetOutputScale {
            name,
            scale_percent,
        } => {
            let Some(scale) = tensor_util::OutputScale::from_f64(f64::from(scale_percent) / 100.0)
            else {
                return IpcReply::new(Response::error(
                    request_id,
                    "invalid_argument",
                    "scale_percent must map to 0.1..=10.0 (100 = 1.0)",
                ));
            };
            return apply_output_ipc(request_id, state, name, |rule| {
                rule.scale = Some(scale);
            });
        }
        IpcCommand::MoveFocusedToWorkspace { index, follow } => {
            if index >= state.workspace_count() {
                return IpcReply::new(Response::error(
                    request_id,
                    "invalid_argument",
                    format!(
                        "workspace index {index} out of range (0..{})",
                        state.workspace_count()
                    ),
                ));
            }
            let Some(view) = state.world.focused_view(state.active_workspace()) else {
                return IpcReply::new(Response::error(
                    request_id,
                    "no_focus",
                    "no focused view on the active workspace",
                ));
            };
            if !state.move_view_to_workspace(view, crate::ecs::WorkspaceId::new(index)) {
                return IpcReply::new(Response::error(
                    request_id,
                    "move_failed",
                    "could not move focused view",
                ));
            }
            if follow {
                let _ = state.activate_workspace_index(index);
            }
            ResultBody::Accepted
        }
        IpcCommand::MinimizeFocused => {
            if state.minimize_focused_view().is_none() {
                return IpcReply::new(Response::error(
                    request_id,
                    "no_focus",
                    "no minimizable focused view on the active workspace",
                ));
            }
            ResultBody::Accepted
        }
        IpcCommand::RestoreMinimized { view, follow } => {
            if !state.restore_minimized_view(crate::ecs::ViewId::new(view), follow) {
                return IpcReply::new(Response::error(
                    request_id,
                    "not_minimized",
                    format!("view {view} is not minimized"),
                ));
            }
            ResultBody::Accepted
        }
        IpcCommand::Quit => {
            return IpcReply::stop_after_flush(Response::new(request_id, ResultBody::Accepted));
        }
    };
    IpcReply::new(Response::new(request_id, result))
}

fn apply_output_ipc(
    request_id: u64,
    state: &mut RuntimeState,
    name: String,
    mutate: impl FnOnce(&mut crate::config::OutputRule),
) -> IpcReply {
    match state.apply_output_rule(name, mutate) {
        Ok(()) => IpcReply::new(Response::new(request_id, ResultBody::Accepted)),
        Err(message) => IpcReply::new(Response::error(request_id, "output_config", message)),
    }
}

fn queue_spawn(
    request_id: u64,
    argv: Vec<String>,
    launch_submitter: &LaunchSubmitter,
    state: &mut RuntimeState,
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
    let token = state.issue_spawn_activation_token().map_err(|error| {
        (
            "activation_token",
            format!("could not issue launch activation token: {error}"),
        )
    })?;
    let request = LaunchRequest::new(
        request_id,
        program.as_str(),
        args.iter().map(String::as_str),
    )
    .with_activation_token(token);
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tensor_runtime::WorkerBridge;

    use super::*;
    use crate::{
        config::{
            Config, ConfigReloadWorker, ConfigTransaction, MAX_PENDING_CONFIG_RELOAD_RESULTS,
        },
        layout::{LayoutEngine, LayoutKind},
        protocol::test_runtime_state,
        scene::SceneAppearance,
        service::SystemdMode,
        spawn::{LaunchWorker, ProcessLauncher},
    };

    #[test]
    fn config_control_queues_reload_and_reports_bounded_status() {
        let path = std::env::temp_dir().join(format!(
            "tensor-ipc-config-missing-{}-{}.kdl",
            std::process::id(),
            line!()
        ));
        let _ = std::fs::remove_file(&path);
        let transaction = ConfigTransaction::new(&path, Config::default());
        let (reload_outcomes, completed_reloads) =
            WorkerBridge::bounded(MAX_PENDING_CONFIG_RELOAD_RESULTS);
        let reload_worker = ConfigReloadWorker::start(path, reload_outcomes).unwrap();
        let reload = reload_worker.submitter();
        let (launch_outcomes, _) = WorkerBridge::bounded(1);
        let launch_worker = LaunchWorker::new(
            ProcessLauncher::with_systemd_detection(SystemdMode::Disabled, false),
            launch_outcomes,
        )
        .unwrap();
        let mut state = test_runtime_state(
            LayoutEngine::new(LayoutKind::Scrolling1D),
            SceneAppearance::default(),
        );
        let config = Some(ConfigIpcContext {
            transaction: &transaction,
            reload: &reload,
        });

        let status = handle_ipc_request_with_config(
            Request::new(90, IpcCommand::GetConfigStatus),
            &mut state,
            &launch_worker.submitter(),
            config,
        );
        assert!(matches!(
            status.response.result,
            ResultBody::ConfigStatus(ConfigStatusSnapshot {
                generation: 0,
                last_failure: None,
            })
        ));

        let queued = handle_ipc_request_with_config(
            Request::new(91, IpcCommand::ReloadConfig),
            &mut state,
            &launch_worker.submitter(),
            config,
        );
        assert!(matches!(queued.response.result, ResultBody::Accepted));
        let outcome = completed_reloads
            .recv_timeout(Duration::from_secs(1))
            .unwrap();
        assert_eq!(outcome.request_id, 91);
        assert!(outcome.candidate.is_err());
    }
}
