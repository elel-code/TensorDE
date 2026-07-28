//! Contracts adapted from Nourish's world, output, and scene tests. Tensor
//! keeps the same invariants while using Bevy ECS stable IDs and retained scene
//! snapshots instead of Nourish's storage tokens.

use tensor_compositor::{
    ecs::{CompositorWorld, ViewId, WorkspaceId},
    layout::{LayoutEngine, LayoutKind, LayoutPlacement, Rect},
    scene::{BackdropBlur, EffectStyle, LinearRgba16, SceneNode, SceneSnapshot, ShadowStyle},
};

const WORKSPACE: WorkspaceId = WorkspaceId::new(9);
const VIEWPORT: Rect = Rect::new(0, 0, 240, 140);

#[test]
fn ecs_lifecycle_and_scene_extraction_keep_ids_separate_from_draw_order() {
    let mut world = CompositorWorld::new();
    for value in [30, 10, 20] {
        world.spawn_view(ViewId::new(value), WORKSPACE).unwrap();
    }
    world.focus_view(ViewId::new(20)).unwrap();
    world.arrange_workspace(
        WORKSPACE,
        LayoutEngine::new(LayoutKind::Spatial2D),
        VIEWPORT,
    );
    let scene = world.extract_scene(WORKSPACE).unwrap();

    assert_eq!(
        scene
            .nodes()
            .iter()
            .map(|node| node.view_id.get())
            .collect::<Vec<_>>(),
        vec![10, 20, 30]
    );
    assert_eq!(
        scene.draw_order().last().unwrap().view_id,
        ViewId::new(20),
        "focus changes stacking order without changing stable node order"
    );

    world.remove_view(ViewId::new(20)).unwrap();
    assert!(world.extract_scene(WORKSPACE).is_none());
    assert_eq!(world.view_count(WORKSPACE), 2);
}

#[test]
fn scene_damage_expands_for_shadow_and_blur_dependencies() {
    let plain = SceneNode::new(
        ViewId::new(1),
        1,
        placement(Rect::new(20, 20, 30, 30)),
        EffectStyle::default(),
    );
    let blur = SceneNode::new(
        ViewId::new(2),
        2,
        placement(Rect::new(0, 0, 200, 100)),
        EffectStyle {
            backdrop_blur: Some(BackdropBlur { radius: 8 }),
            ..EffectStyle::default()
        },
    );
    let old = SceneSnapshot::new(WORKSPACE, VIEWPORT, vec![plain, blur]);
    let moved = SceneNode::new(
        ViewId::new(1),
        1,
        placement(Rect::new(70, 20, 30, 30)),
        EffectStyle {
            shadow: Some(ShadowStyle {
                offset_x: 2,
                offset_y: 1,
                blur_radius: 4,
                spread: 2,
                color: LinearRgba16::new(0, 0, 0, u16::MAX),
            }),
            ..EffectStyle::default()
        },
    );
    let new = SceneSnapshot::new(WORKSPACE, VIEWPORT, vec![moved, blur]);

    assert_eq!(
        new.damage_since(Some(&old)).regions(),
        &[Rect::new(0, 0, 200, 100)]
    );
}

#[test]
fn scene_geometry_is_independent_of_input_node_order() {
    let first = SceneSnapshot::new(
        WORKSPACE,
        VIEWPORT,
        vec![
            SceneNode::new(
                ViewId::new(2),
                1,
                placement(Rect::new(10, 10, 20, 20)),
                EffectStyle::default(),
            ),
            SceneNode::new(
                ViewId::new(1),
                2,
                placement(Rect::new(40, 10, 20, 20)),
                EffectStyle::default(),
            ),
        ],
    );
    let second = SceneSnapshot::new(
        WORKSPACE,
        VIEWPORT,
        vec![
            SceneNode::new(
                ViewId::new(1),
                2,
                placement(Rect::new(40, 10, 20, 20)),
                EffectStyle::default(),
            ),
            SceneNode::new(
                ViewId::new(2),
                1,
                placement(Rect::new(10, 10, 20, 20)),
                EffectStyle::default(),
            ),
        ],
    );

    assert_eq!(first.nodes(), second.nodes());
    assert_eq!(
        first
            .draw_order()
            .map(|node| node.view_id)
            .collect::<Vec<_>>(),
        second
            .draw_order()
            .map(|node| node.view_id)
            .collect::<Vec<_>>()
    );
}

fn placement(geometry: Rect) -> LayoutPlacement {
    LayoutPlacement {
        geometry,
        visible: geometry.intersection(VIEWPORT),
    }
}
