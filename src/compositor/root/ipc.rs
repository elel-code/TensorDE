//! Compositor-side handling for values produced by completed IPC operations.

use std::{cell::RefCell, rc::Rc};

use calloop::LoopSignal;
use tensor_runtime::WorkerRx;
use tracing::{error, info};

use crate::{
    ipc::{
        Command as IpcCommand, IPC_PROTOCOL_VERSION, IpcControlEvent, IpcEvent, IpcReply,
        MAX_PENDING_IPC_CONTROL_EVENTS, MAX_PENDING_IPC_REQUESTS, Request, Response, ResultBody,
    },
    layout::LayoutEngine,
    protocol::RuntimeState,
    spawn::{LaunchRequest, LaunchSubmitError, LaunchSubmitter},
};

pub(super) fn drain_ipc_events(
    requests: &WorkerRx<IpcEvent>,
    control: &WorkerRx<IpcControlEvent>,
    state: &mut RuntimeState,
    stop_signal: &LoopSignal,
    launch_submitter: &LaunchSubmitter,
    runtime_failure: &Rc<RefCell<Option<String>>>,
) {
    requests.drain(
        MAX_PENDING_IPC_REQUESTS,
        |IpcEvent {
             request,
             respond_to,
         }| {
            let reflow = matches!(&request.command, crate::ipc::Command::SetLayout { .. });
            let reply = handle_ipc_request(request, state, launch_submitter);
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
            runtime_failure.borrow_mut().replace(message);
            stop_signal.stop();
        }
    });
}

pub(super) fn handle_ipc_request(
    request: Request,
    state: &mut RuntimeState,
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
        IpcCommand::GetWorkspaces => ResultBody::Workspaces(state.ipc_workspace_snapshots()),
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
    let token = state.issue_spawn_activation_token();
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
