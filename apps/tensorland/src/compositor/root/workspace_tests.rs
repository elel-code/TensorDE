use tensor_runtime::{WorkerBridge, WorkerRx};

use super::{LaunchOutcome, LaunchWorker, ProcessLauncher, ipc::handle_ipc_request};
use crate::{
    ecs::{ViewId, ViewPlacement, WorkspaceId},
    ipc::{Command, Request, ResultBody},
    layout::{LayoutEngine, LayoutKind},
    protocol::RuntimeState,
    scene::SceneAppearance,
    service::SystemdMode,
};
use tensor_util::Size;

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
fn ipc_minimize_moves_to_hidden_workspace_and_restores_origin() {
    let mut state = runtime_state();
    let view = ViewId::new(41);
    let dialog = ViewId::new(42);
    state.world.spawn_view(view, WorkspaceId::new(0)).unwrap();
    state.world.spawn_view(dialog, WorkspaceId::new(0)).unwrap();
    state
        .world
        .set_view_placement(
            dialog,
            ViewPlacement::Attached {
                owner: view,
                preferred_size: Size::new(320, 200),
            },
        )
        .unwrap();
    state.world.focus_view(dialog).unwrap();
    let (worker, _) = live_worker();
    let submitter = worker.submitter();

    let minimized = handle_ipc_request(
        Request::new(20, Command::MinimizeFocused),
        &mut state,
        &submitter,
    );
    assert!(matches!(minimized.response.result, ResultBody::Accepted));
    assert_eq!(state.world.view_workspace(view), Some(WorkspaceId::new(1)));
    assert_eq!(
        state.world.view_workspace(dialog),
        Some(WorkspaceId::new(1))
    );
    assert_eq!(state.ipc_state_snapshot().minimized_count, 1);

    let restored = handle_ipc_request(
        Request::new(
            21,
            Command::RestoreMinimized {
                view: dialog.get(),
                follow: true,
            },
        ),
        &mut state,
        &submitter,
    );
    assert!(matches!(restored.response.result, ResultBody::Accepted));
    assert_eq!(state.world.view_workspace(view), Some(WorkspaceId::new(0)));
    assert_eq!(
        state.world.view_workspace(dialog),
        Some(WorkspaceId::new(0))
    );
    assert_eq!(state.ipc_state_snapshot().minimized_count, 0);
    drop(worker);
}
