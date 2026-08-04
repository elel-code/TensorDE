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

    let overview = handle_ipc_request(
        Request::new(22, Command::GetOverview),
        &mut state,
        &submitter,
    );
    let ResultBody::Overview(overview) = overview.response.result else {
        panic!("expected overview inventory");
    };
    assert!(!overview.truncated);
    assert_eq!(overview.workspaces.len(), 2);
    let minimized = &overview.workspaces[1];
    assert!(minimized.hidden);
    assert!(minimized.minimize_target);
    assert_eq!(minimized.view_count, 2);
    assert_eq!(minimized.views.len(), 2);
    assert!(minimized.views.iter().all(|view| view.root == 41));

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

#[test]
fn overview_inventory_filters_hidden_workspace_policy() {
    let mut state = runtime_state();
    let mut config = crate::config::WorkspaceConfig::default();
    config.hidden[0].show_in_overview = false;
    state.configure_workspaces(&config);

    let overview = state.ipc_overview_snapshot();

    assert_eq!(overview.workspaces.len(), 1);
    assert!(!overview.workspaces[0].hidden);
}

#[test]
fn overview_inventory_is_a_bounded_stable_prefix() {
    let mut state = runtime_state();
    for id in 1..=crate::ipc::MAX_OVERVIEW_VIEWS as u64 + 1 {
        state
            .world
            .spawn_view(ViewId::new(id), WorkspaceId::new(0))
            .unwrap();
    }

    let overview = state.ipc_overview_snapshot();

    assert!(overview.truncated);
    assert_eq!(overview.workspaces[0].view_count, 4_097);
    assert_eq!(
        overview.workspaces[0].views.len(),
        crate::ipc::MAX_OVERVIEW_VIEWS
    );
    assert_eq!(overview.workspaces[0].views[0].id, 1);
    let response = crate::ipc::Response::new(1, ResultBody::Overview(overview));
    assert!(crate::ipc::encode(&response).is_ok());
}
