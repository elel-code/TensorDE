use crate::{
    layout::{LayoutEngine, LayoutKind},
    scene::SceneAppearance,
};
use tensor_runtime::WorkerBridge;

use super::super::{ProtocolError, WaylandRuntime};

fn runtime() -> WaylandRuntime {
    WaylandRuntime::with_appearance(
        LayoutEngine::new(LayoutKind::Scrolling1D),
        SceneAppearance::default(),
    )
    .unwrap()
}

pub(super) fn assert_workspace_visibility_retains_window(
    runtime: &mut WaylandRuntime,
) -> Option<crate::ecs::ViewId> {
    let view_id = runtime.state.view_for_surface(
        runtime
            .state
            .space
            .elements()
            .next()
            .unwrap()
            .wl_surface()
            .as_deref()
            .unwrap(),
    );
    assert_eq!(
        view_id.and_then(|view_id| runtime.state.world.view_layout(view_id)),
        Some(crate::ecs::ViewLayout {
            constraints: crate::layout::SizeConstraints::new(
                tensor_util::Size::new(320, 200),
                Some(640),
                Some(480),
            ),
            primary_size: None,
        })
    );
    let workspace_config = crate::config::WorkspaceConfig {
        regular_count: 2,
        ..Default::default()
    };
    runtime.state.configure_workspaces(&workspace_config);
    assert!(runtime.state.activate_workspace_index(1));
    assert_eq!(runtime.state.space.elements().count(), 0);
    assert_eq!(runtime.state.space.retained_elements().count(), 1);
    assert!(runtime.state.activate_workspace_index(0));
    assert_eq!(runtime.state.space.elements().count(), 1);
    assert_eq!(runtime.state.space.retained_elements().count(), 1);
    let overview = runtime.state.ipc_overview_snapshot();
    let overview_view = &overview.workspaces[0].views[0];
    assert_eq!(overview_view.id, view_id.unwrap().get());
    assert_eq!(overview_view.root, overview_view.id);
    assert!(overview_view.source_geometry.is_some());
    assert_eq!(overview_view.geometry.is_some(), overview.area.is_some());
    assert!(
        overview_view
            .foreign_toplevel_identifier
            .as_deref()
            .is_some_and(|identifier| identifier.starts_with("tensor-"))
    );
    view_id
}

#[test]
fn completion_runtime_installation_is_single_shot() {
    let mut runtime = runtime();
    let _relay = runtime.prepare_for_test(false).unwrap();

    let (clients, _client_events) = WorkerBridge::bounded(1);
    let (socket_control, _socket_failures) = WorkerBridge::bounded(1);
    assert!(matches!(
        runtime.install_socket_runtime(clients, socket_control),
        Err(ProtocolError::SocketRuntimeAlreadyInstalled)
    ));

    let (display, _display_events) = WorkerBridge::bounded(1);
    let (display_control, _display_failures) = WorkerBridge::bounded(1);
    assert!(matches!(
        runtime.install_display_runtime(display, display_control),
        Err(ProtocolError::DisplayRuntimeAlreadyInstalled)
    ));
}

#[test]
fn runtime_preparation_is_single_shot() {
    let mut runtime = runtime();
    let _relay = runtime.prepare_for_test(false).unwrap();
    assert!(matches!(
        runtime.prepare(false),
        Err(ProtocolError::RuntimeAlreadyPrepared)
    ));
}

#[cfg(feature = "xwayland")]
#[test]
fn xwayland_completion_channel_installation_is_single_shot() {
    let mut runtime = runtime();
    let (events, _event_rx) = WorkerBridge::bounded(1);
    let (control, _control_rx) = WorkerBridge::bounded(1);
    let (properties, _property_rx) = WorkerBridge::bounded(1);
    let (property_control, _property_control_rx) = WorkerBridge::bounded(1);
    runtime
        .install_xwayland_completion_channels(events, control, properties, property_control)
        .unwrap();

    let (events, _event_rx) = WorkerBridge::bounded(1);
    let (control, _control_rx) = WorkerBridge::bounded(1);
    let (properties, _property_rx) = WorkerBridge::bounded(1);
    let (property_control, _property_control_rx) = WorkerBridge::bounded(1);
    assert!(matches!(
        runtime
            .install_xwayland_completion_channels(events, control, properties, property_control,),
        Err(ProtocolError::XWaylandCompletionChannelsAlreadyInstalled)
    ));
}
