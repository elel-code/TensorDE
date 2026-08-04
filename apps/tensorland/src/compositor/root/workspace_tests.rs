use tensor_runtime::{WorkerBridge, WorkerRx};

use super::{LaunchOutcome, LaunchWorker, ProcessLauncher, ipc::handle_ipc_request};
use crate::{
    config::WorkspaceConfig,
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

fn state_with_two_workspaces() -> RuntimeState {
    let mut state = runtime_state();
    state.configure_workspaces(&WorkspaceConfig {
        regular_count: 2,
        ..WorkspaceConfig::default()
    });
    state
}

fn assert_error_code(result: ResultBody, expected: &str) {
    let ResultBody::Error(error) = result else {
        panic!("expected IPC error {expected}");
    };
    assert_eq!(error.code, expected);
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
fn ipc_activate_view_selects_the_requested_attached_dialog() {
    let mut state = state_with_two_workspaces();
    let root = ViewId::new(51);
    let dialog = ViewId::new(52);
    state.world.spawn_view(root, WorkspaceId::new(1)).unwrap();
    state.world.spawn_view(dialog, WorkspaceId::new(1)).unwrap();
    state
        .world
        .set_view_placement(
            dialog,
            ViewPlacement::Attached {
                owner: root,
                preferred_size: Size::new(320, 200),
            },
        )
        .unwrap();
    let (worker, _) = live_worker();

    let reply = handle_ipc_request(
        Request::new(30, Command::ActivateView { view: dialog.get() }),
        &mut state,
        &worker.submitter(),
    );

    assert!(matches!(reply.response.result, ResultBody::Accepted));
    assert_eq!(state.active_workspace(), WorkspaceId::new(1));
    assert_eq!(state.world.focused_view(WorkspaceId::new(1)), Some(dialog));
}

#[test]
fn ipc_activate_view_restores_a_minimized_dialog_family() {
    let mut state = runtime_state();
    let root = ViewId::new(61);
    let dialog = ViewId::new(62);
    state.world.spawn_view(root, WorkspaceId::new(0)).unwrap();
    state.world.spawn_view(dialog, WorkspaceId::new(0)).unwrap();
    state
        .world
        .set_view_placement(
            dialog,
            ViewPlacement::Attached {
                owner: root,
                preferred_size: Size::new(320, 200),
            },
        )
        .unwrap();
    state.world.focus_view(dialog).unwrap();
    assert_eq!(state.minimize_focused_view(), Some(root));
    let (worker, _) = live_worker();

    let reply = handle_ipc_request(
        Request::new(31, Command::ActivateView { view: dialog.get() }),
        &mut state,
        &worker.submitter(),
    );

    assert!(matches!(reply.response.result, ResultBody::Accepted));
    assert_eq!(state.world.view_workspace(root), Some(WorkspaceId::new(0)));
    assert_eq!(
        state.world.view_workspace(dialog),
        Some(WorkspaceId::new(0))
    );
    assert_eq!(state.world.focused_view(WorkspaceId::new(0)), Some(dialog));
    assert_eq!(state.ipc_state_snapshot().minimized_count, 0);
}

#[test]
fn ipc_activate_view_returns_structured_identity_and_hidden_errors() {
    let mut state = runtime_state();
    let hidden = ViewId::new(71);
    state.world.spawn_view(hidden, WorkspaceId::new(1)).unwrap();
    let (worker, _) = live_worker();
    let submitter = worker.submitter();

    let unknown = handle_ipc_request(
        Request::new(32, Command::ActivateView { view: 999 }),
        &mut state,
        &submitter,
    );
    assert_error_code(unknown.response.result, "unknown_view");

    let hidden = handle_ipc_request(
        Request::new(33, Command::ActivateView { view: hidden.get() }),
        &mut state,
        &submitter,
    );
    assert_error_code(hidden.response.result, "hidden_workspace");
}

#[test]
fn ipc_move_view_moves_the_family_and_follow_focuses_the_requested_dialog() {
    let mut state = state_with_two_workspaces();
    let root = ViewId::new(81);
    let dialog = ViewId::new(82);
    state.world.spawn_view(root, WorkspaceId::new(0)).unwrap();
    state.world.spawn_view(dialog, WorkspaceId::new(0)).unwrap();
    state
        .world
        .set_view_placement(
            dialog,
            ViewPlacement::Attached {
                owner: root,
                preferred_size: Size::new(320, 200),
            },
        )
        .unwrap();
    let (worker, _) = live_worker();
    let submitter = worker.submitter();

    let invalid = handle_ipc_request(
        Request::new(
            34,
            Command::MoveViewToWorkspace {
                view: dialog.get(),
                index: 2,
                follow: false,
            },
        ),
        &mut state,
        &submitter,
    );
    assert_error_code(invalid.response.result, "invalid_argument");

    let moved = handle_ipc_request(
        Request::new(
            35,
            Command::MoveViewToWorkspace {
                view: dialog.get(),
                index: 1,
                follow: true,
            },
        ),
        &mut state,
        &submitter,
    );

    assert!(matches!(moved.response.result, ResultBody::Accepted));
    assert_eq!(state.world.view_workspace(root), Some(WorkspaceId::new(1)));
    assert_eq!(
        state.world.view_workspace(dialog),
        Some(WorkspaceId::new(1))
    );
    assert_eq!(state.active_workspace(), WorkspaceId::new(1));
    assert_eq!(state.world.focused_view(WorkspaceId::new(1)), Some(dialog));
}

#[test]
fn ipc_close_view_reports_unknown_and_unmapped_stable_ids() {
    let mut state = runtime_state();
    let ghost = ViewId::new(91);
    state.world.spawn_view(ghost, WorkspaceId::new(0)).unwrap();
    let (worker, _) = live_worker();
    let submitter = worker.submitter();

    let unknown = handle_ipc_request(
        Request::new(36, Command::CloseView { view: 999 }),
        &mut state,
        &submitter,
    );
    assert_error_code(unknown.response.result, "unknown_view");

    let unmapped = handle_ipc_request(
        Request::new(37, Command::CloseView { view: ghost.get() }),
        &mut state,
        &submitter,
    );
    assert_error_code(unmapped.response.result, "unmapped_view");
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
    assert_eq!(
        overview.workspaces[0].view_count,
        crate::ipc::MAX_OVERVIEW_VIEWS + 1
    );
    assert_eq!(
        overview.workspaces[0].views.len(),
        crate::ipc::MAX_OVERVIEW_VIEWS
    );
    assert_eq!(overview.workspaces[0].views[0].id, 1);
    let response = crate::ipc::Response::new(1, ResultBody::Overview(overview));
    assert!(crate::ipc::encode(&response).is_ok());
}
