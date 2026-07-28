//! Contracts adapted from `references/tensor/niri/src/tests/window_opening.rs`,
//! `transactions.rs`, and `remove_output.rs`.

use tensor_compositor::{
    ecs::{CompositorWorld, ViewId, ViewLayout, WorkspaceId},
    layout::{LayoutEngine, LayoutKind, LayoutLength, Rect, SizeConstraints},
    scene::SceneSnapshot,
};
use tensor_util::Size;

const WORKSPACE: WorkspaceId = WorkspaceId::new(1);
const VIEWPORT: Rect = Rect::new(0, 0, 1000, 800);

#[test]
fn window_opening_constraints_drive_geometry_and_scene_together() {
    let view = ViewId::new(7);
    let mut world = CompositorWorld::new();
    world.spawn_view(view, WORKSPACE).unwrap();
    world
        .set_view_layout(
            view,
            ViewLayout {
                constraints: SizeConstraints::new(Size::new(320, 200), Some(640), Some(480)),
                primary_size: Some(LayoutLength::fixed(700)),
            },
        )
        .unwrap();

    let layout = LayoutEngine::new(LayoutKind::Scrolling1D);
    let snapshot = world.arrange_workspace(WORKSPACE, layout, VIEWPORT).clone();
    let geometry = world.geometry(view).expect("arranged view has geometry");

    // The configure-like size is constrained before it reaches both Space/ECS
    // geometry and scene extraction: 700 is reduced to the 640 maximum.
    assert_eq!(geometry, Rect::new(8, 160, 640, 480));
    assert_eq!(snapshot.placements[0].geometry, geometry);

    let scene = world
        .extract_scene(WORKSPACE)
        .expect("an arranged workspace extracts a scene");
    assert_eq!(scene.nodes().len(), 1);
    assert_eq!(scene.nodes()[0].placement.geometry, geometry);
}

#[test]
fn focused_window_reveals_only_the_needed_scrolling_extent() {
    let mut world = CompositorWorld::new();
    for value in 1..=3 {
        world.spawn_view(ViewId::new(value), WORKSPACE).unwrap();
    }
    world.focus_view(ViewId::new(3)).unwrap();

    let snapshot = world
        .arrange_workspace(
            WORKSPACE,
            LayoutEngine::new(LayoutKind::Scrolling1D),
            Rect::new(0, 0, 100, 80),
        )
        .clone();

    assert_eq!(snapshot.horizontal_offset, -46);
    assert_eq!(
        world.geometry(ViewId::new(3)),
        Some(Rect::new(54, 8, 38, 64))
    );
    assert_eq!(
        snapshot.placements[2].visible,
        Some(Rect::new(54, 8, 38, 64))
    );
}

#[test]
fn removing_a_view_invalidates_only_its_workspace_snapshot() {
    let other_workspace = WorkspaceId::new(2);
    let mut world = CompositorWorld::new();
    world.spawn_view(ViewId::new(1), WORKSPACE).unwrap();
    world.spawn_view(ViewId::new(2), other_workspace).unwrap();
    world.arrange_workspace(
        WORKSPACE,
        LayoutEngine::new(LayoutKind::Scrolling1D),
        VIEWPORT,
    );
    world.arrange_workspace(
        other_workspace,
        LayoutEngine::new(LayoutKind::Scrolling1D),
        VIEWPORT,
    );
    assert!(world.layout_snapshot(WORKSPACE).is_some());
    assert!(world.layout_snapshot(other_workspace).is_some());

    world.remove_view(ViewId::new(1)).unwrap();

    assert!(world.layout_snapshot(WORKSPACE).is_none());
    assert!(world.layout_snapshot(other_workspace).is_some());
    assert_eq!(world.view_count(WORKSPACE), 0);
    assert_eq!(world.view_count(other_workspace), 1);
}

#[test]
fn an_unchanged_configure_result_has_no_followup_damage() {
    let scene = SceneSnapshot::new(WORKSPACE, VIEWPORT, Vec::new());
    assert_eq!(scene.damage_since(None).regions(), &[VIEWPORT]);
    assert!(scene.damage_since(Some(&scene)).regions().is_empty());
}
