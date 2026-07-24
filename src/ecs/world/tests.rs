use super::*;
use crate::layout::LayoutKind;
use tensor_util::Size;

fn view(value: u64) -> ViewId {
    ViewId::new(value)
}

fn workspace(value: u32) -> WorkspaceId {
    WorkspaceId::new(value)
}

#[test]
fn layout_system_uses_stable_view_ids() {
    let mut world = CompositorWorld::new();
    world.spawn_view(view(20), workspace(1)).unwrap();
    world.spawn_view(view(10), workspace(1)).unwrap();

    world.arrange_workspace(
        workspace(1),
        LayoutEngine::new(LayoutKind::Scrolling1D),
        Rect::new(0, 0, 100, 80),
    );

    assert_eq!(world.geometry(view(10)), Some(Rect::new(8, 8, 38, 64)));
    assert_eq!(world.geometry(view(20)), Some(Rect::new(54, 8, 38, 64)));
}

#[test]
fn focused_view_drives_workspace_local_scrolling_state() {
    let mut world = CompositorWorld::new();
    for id in 1..=3 {
        world.spawn_view(view(id), workspace(1)).unwrap();
    }
    world.focus_view(view(3)).unwrap();

    let snapshot = world.arrange_workspace(
        workspace(1),
        LayoutEngine::new(LayoutKind::Scrolling1D),
        Rect::new(0, 0, 100, 80),
    );

    assert_eq!(snapshot.horizontal_offset, -46);
    assert_eq!(world.geometry(view(3)), Some(Rect::new(54, 8, 38, 64)));
}

#[test]
fn restoring_existing_focus_keeps_the_cached_scene_renderable() {
    let mut world = CompositorWorld::new();
    world.spawn_view(view(1), workspace(1)).unwrap();
    world.focus_view(view(1)).unwrap();
    world.arrange_workspace(
        workspace(1),
        LayoutEngine::new(LayoutKind::Scrolling1D),
        Rect::new(0, 0, 100, 80),
    );

    // `RuntimeState::restore_keyboard_focus` reaches this path when libinput
    // publishes a keyboard after an already-mapped application. It needs a
    // wl_keyboard.enter, not a new ECS scene mutation.
    world.focus_view(view(1)).unwrap();

    let scene = world
        .extract_scene(workspace(1))
        .expect("restoring focus must not drop the frame scene");
    assert_eq!(
        scene.nodes()[0].focus_outline,
        Some(crate::scene::FocusOutline::DEFAULT)
    );
}

#[test]
fn scene_appearance_controls_the_focused_view_ring_without_touching_view_state() {
    let appearance = crate::scene::SceneAppearance {
        focus_ring: crate::scene::FocusRingStyle {
            enabled: true,
            width: 7,
            color: crate::scene::LinearRgba16::new(0x1111, 0x2222, 0x3333, u16::MAX),
        },
    };
    let mut world = CompositorWorld::with_appearance(appearance);
    world.spawn_view(view(1), workspace(1)).unwrap();
    world.focus_view(view(1)).unwrap();
    world.arrange_workspace(
        workspace(1),
        LayoutEngine::new(LayoutKind::Scrolling1D),
        Rect::new(0, 0, 100, 80),
    );

    let scene = world.extract_scene(workspace(1)).unwrap();
    assert_eq!(
        scene.nodes()[0].focus_outline,
        appearance.focus_ring.outline()
    );

    assert!(world.set_appearance(crate::scene::SceneAppearance {
        focus_ring: crate::scene::FocusRingStyle {
            enabled: false,
            ..appearance.focus_ring
        },
    }));
    assert!(!world.set_appearance(world.appearance()));
    assert_eq!(
        world.extract_scene(workspace(1)).unwrap().nodes()[0].focus_outline,
        None
    );
}

#[test]
fn per_view_constraints_flow_into_layout_geometry() {
    use crate::layout::{LayoutLength, SizeConstraints};

    let mut world = CompositorWorld::new();
    world.spawn_view(view(1), workspace(1)).unwrap();
    world
        .set_view_layout(
            view(1),
            ViewLayout {
                constraints: SizeConstraints::new(Size::new(1, 1), Some(200), Some(100)),
                primary_size: Some(LayoutLength::fixed(400)),
            },
        )
        .unwrap();

    world.arrange_workspace(
        workspace(1),
        LayoutEngine::new(LayoutKind::Spatial2D),
        Rect::new(0, 0, 500, 300),
    );

    assert_eq!(world.geometry(view(1)), Some(Rect::new(150, 100, 200, 100)));
}

#[test]
fn protocol_constraints_preserve_the_configured_primary_size() {
    use crate::layout::{LayoutLength, SizeConstraints};

    let mut world = CompositorWorld::new();
    world.spawn_view(view(1), workspace(1)).unwrap();
    world
        .set_view_layout(
            view(1),
            ViewLayout {
                constraints: SizeConstraints::default(),
                primary_size: Some(LayoutLength::fixed(420)),
            },
        )
        .unwrap();

    let constraints = SizeConstraints::new(Size::new(200, 100), Some(800), Some(600));
    assert!(world.set_view_constraints(view(1), constraints).unwrap());
    assert!(!world.set_view_constraints(view(1), constraints).unwrap());

    assert_eq!(
        world.view_layout(view(1)),
        Some(ViewLayout {
            constraints,
            primary_size: Some(LayoutLength::fixed(420)),
        })
    );
}

#[test]
fn layout_snapshot_is_shared_and_invalidated_by_scene_changes() {
    let mut world = CompositorWorld::new();
    world.spawn_view(view(1), workspace(1)).unwrap();
    world.arrange_workspace(
        workspace(1),
        LayoutEngine::new(LayoutKind::Scrolling1D),
        Rect::new(0, 0, 100, 80),
    );

    assert_eq!(
        world.layout_snapshot(workspace(1)).unwrap().placements[0].geometry,
        Rect::new(8, 8, 38, 64)
    );

    world.spawn_view(view(2), workspace(1)).unwrap();
    assert_eq!(world.layout_snapshot(workspace(1)), None);
}

#[test]
fn scene_extraction_separates_stable_nodes_from_draw_order() {
    use crate::scene::{LinearRgba16, ShadowStyle};

    let mut world = CompositorWorld::new();
    world.spawn_view(view(2), workspace(1)).unwrap();
    world.spawn_view(view(1), workspace(1)).unwrap();
    world.focus_view(view(2)).unwrap();
    let effects = EffectStyle {
        corner_radius: 12,
        shadow: Some(ShadowStyle {
            offset_x: 2,
            offset_y: 3,
            blur_radius: 8,
            spread: 1,
            color: LinearRgba16::new(0, 0, 0, 32_768),
        }),
        ..Default::default()
    };
    assert!(world.set_view_effects(view(2), effects).unwrap());
    assert!(!world.set_view_effects(view(2), effects).unwrap());
    world.arrange_workspace(
        workspace(1),
        LayoutEngine::new(LayoutKind::Scrolling1D),
        Rect::new(0, 0, 100, 80),
    );

    let scene = world.extract_scene(workspace(1)).unwrap();

    assert_eq!(
        scene
            .nodes()
            .iter()
            .map(|node| node.view_id)
            .collect::<Vec<_>>(),
        [view(1), view(2)]
    );
    assert_eq!(
        scene
            .draw_order()
            .map(|node| node.view_id)
            .collect::<Vec<_>>(),
        [view(1), view(2)]
    );
    assert_eq!(scene.nodes()[1].effects, effects);
}

#[test]
fn scene_extraction_keeps_surface_content_out_of_smithay_and_entity_ids() {
    use crate::scene::{ContentRevision, SurfaceContent, SurfaceLayer, SurfaceTransform};

    let mut world = CompositorWorld::new();
    world.spawn_view(view(1), workspace(1)).unwrap();
    let content = SurfaceContent {
        surface_id: crate::ecs::SurfaceId::new(7),
        buffer_id: crate::ecs::SurfaceBufferId::new(9),
        revision: ContentRevision::new(3),
        layer: SurfaceLayer::View,
        buffer_size: Size::new(640, 480),
        local_geometry: Rect::new(0, 0, 640, 480),
        buffer_scale: 1,
        transform: SurfaceTransform::Normal,
    };
    assert!(world.set_view_content(view(1), vec![content]).unwrap());
    assert_eq!(world.view_content(view(1)).unwrap().surfaces, [content]);
    world.arrange_workspace(
        workspace(1),
        LayoutEngine::new(LayoutKind::Scrolling1D),
        Rect::new(0, 0, 100, 80),
    );

    let scene = world.extract_scene(workspace(1)).unwrap();
    assert_eq!(scene.contents(), [content]);
    assert_eq!(scene.contents_for(&scene.nodes()[0]), [content]);
}

#[test]
fn duplicate_view_ids_are_rejected_without_replacing_the_original() {
    let mut world = CompositorWorld::new();
    world.spawn_view(view(7), workspace(1)).unwrap();

    assert_eq!(
        world.spawn_view(view(7), workspace(2)),
        Err(ViewLifecycleError::DuplicateViewId(view(7)))
    );
    assert_eq!(world.view_count(workspace(1)), 1);
    assert_eq!(world.view_count(workspace(2)), 0);
}

#[test]
fn focus_is_unique_per_workspace_and_survives_workspace_moves() {
    let mut world = CompositorWorld::new();
    world.spawn_view(view(1), workspace(1)).unwrap();
    world.spawn_view(view(2), workspace(1)).unwrap();
    world.spawn_view(view(3), workspace(2)).unwrap();

    world.focus_view(view(1)).unwrap();
    world.focus_view(view(2)).unwrap();
    assert!(!world.is_focused(view(1)));
    assert!(world.is_focused(view(2)));
    assert_eq!(world.focused_view(workspace(1)), Some(view(2)));

    world.arrange_workspace(
        workspace(1),
        LayoutEngine::new(LayoutKind::Scrolling1D),
        Rect::new(0, 0, 100, 80),
    );
    assert!(world.geometry(view(2)).is_some());
    world.focus_view(view(3)).unwrap();
    world.move_view(view(2), workspace(2)).unwrap();
    assert_eq!(world.focused_view(workspace(1)), None);
    assert_eq!(world.focused_view(workspace(2)), Some(view(2)));
    assert!(world.is_focused(view(2)));
    assert!(!world.is_focused(view(3)));
    assert_eq!(world.geometry(view(2)), None);
}

#[test]
fn removed_views_release_their_stable_id() {
    let mut world = CompositorWorld::new();
    world.spawn_view(view(9), workspace(1)).unwrap();
    world.focus_view(view(9)).unwrap();
    world.remove_view(view(9)).unwrap();

    assert_eq!(world.geometry(view(9)), None);
    assert_eq!(world.focused_view(workspace(1)), None);
    assert_eq!(world.spawn_view(view(9), workspace(2)), Ok(()));
}

#[test]
fn attached_view_keeps_an_independent_scene_node_out_of_tile_allocation() {
    let mut world = CompositorWorld::new();
    world.spawn_view(view(1), workspace(1)).unwrap();
    world.spawn_view(view(2), workspace(1)).unwrap();
    world.spawn_view(view(3), workspace(1)).unwrap();
    assert!(
        world
            .set_view_placement(
                view(3),
                ViewPlacement::Attached {
                    owner: view(1),
                    preferred_size: Size::new(30, 20),
                },
            )
            .unwrap()
    );

    let snapshot = world
        .arrange_workspace(
            workspace(1),
            LayoutEngine::new(LayoutKind::Spatial2D),
            Rect::new(0, 0, 200, 100),
        )
        .clone();
    let owner = world.geometry(view(1)).unwrap();

    assert_eq!(snapshot.placements.len(), 2);
    assert_eq!(
        world.geometry(view(3)),
        Some(Rect::new(
            owner.x + (i32::try_from(owner.width).unwrap() - 30) / 2,
            owner.y + (i32::try_from(owner.height).unwrap() - 20) / 2,
            30,
            20,
        ))
    );
    let scene = world.extract_scene(workspace(1)).unwrap();
    assert_eq!(scene.nodes().len(), 3);
    assert_eq!(
        scene.nodes()[2].placement.geometry,
        world.geometry(view(3)).unwrap()
    );
}

#[test]
fn attached_focus_reveals_the_tiled_owner_and_raises_its_subtree() {
    let mut world = CompositorWorld::new();
    for id in 1..=3 {
        world.spawn_view(view(id), workspace(1)).unwrap();
    }
    world.spawn_view(view(4), workspace(1)).unwrap();
    world
        .set_view_placement(
            view(4),
            ViewPlacement::Attached {
                owner: view(3),
                preferred_size: Size::new(20, 20),
            },
        )
        .unwrap();
    world.focus_view(view(4)).unwrap();

    let snapshot = world
        .arrange_workspace(
            workspace(1),
            LayoutEngine::new(LayoutKind::Scrolling1D),
            Rect::new(0, 0, 100, 80),
        )
        .clone();

    assert_eq!(snapshot.horizontal_offset, -46);
    assert!(world.is_focused(view(4)));
    assert!(!world.is_focused(view(3)));
    assert_eq!(world.tiled_ancestor(view(4)), Some(view(3)));
    let scene = world.extract_scene(workspace(1)).unwrap();
    assert_eq!(scene.draw_order().last().unwrap().view_id, view(4));
}

#[test]
fn attachment_invariants_reject_cycles_cross_workspace_and_orphaned_removal() {
    let mut world = CompositorWorld::new();
    world.spawn_view(view(1), workspace(1)).unwrap();
    world.spawn_view(view(2), workspace(1)).unwrap();
    world.spawn_view(view(3), workspace(2)).unwrap();

    assert_eq!(
        world.set_view_placement(
            view(1),
            ViewPlacement::Attached {
                owner: view(1),
                preferred_size: Size::new(1, 1),
            },
        ),
        Err(ViewLifecycleError::SelfAttachment(view(1)))
    );
    assert_eq!(
        world.set_view_placement(
            view(1),
            ViewPlacement::Attached {
                owner: view(3),
                preferred_size: Size::new(1, 1),
            },
        ),
        Err(ViewLifecycleError::CrossWorkspaceAttachment {
            view: view(1),
            owner: view(3),
        })
    );
    world
        .set_view_placement(
            view(2),
            ViewPlacement::Attached {
                owner: view(1),
                preferred_size: Size::new(1, 1),
            },
        )
        .unwrap();
    assert_eq!(
        world.set_view_placement(
            view(1),
            ViewPlacement::Attached {
                owner: view(2),
                preferred_size: Size::new(1, 1),
            },
        ),
        Err(ViewLifecycleError::AttachmentCycle {
            view: view(1),
            owner: view(2),
        })
    );
    assert_eq!(
        world.remove_view(view(1)),
        Err(ViewLifecycleError::AttachedChild {
            owner: view(1),
            child: view(2),
        })
    );
    assert_eq!(
        world.move_view(view(2), workspace(2)),
        Err(ViewLifecycleError::AttachedViewCannotMove(view(2)))
    );
}

#[test]
fn moving_a_tiled_owner_moves_its_attached_dialog_family() {
    let first = workspace(1);
    let second = workspace(2);
    let mut world = CompositorWorld::new();
    world.spawn_view(view(1), first).unwrap();
    world.spawn_view(view(2), first).unwrap();
    world.spawn_view(view(3), second).unwrap();
    world
        .set_view_placement(
            view(2),
            ViewPlacement::Attached {
                owner: view(1),
                preferred_size: Size::new(40, 30),
            },
        )
        .unwrap();
    world.focus_view(view(2)).unwrap();

    world.move_view(view(1), second).unwrap();

    assert_eq!(world.view_count(first), 0);
    assert_eq!(world.view_count(second), 3);
    assert_eq!(world.focused_view(first), None);
    assert_eq!(world.focused_view(second), Some(view(2)));
    world.arrange_workspace(
        second,
        LayoutEngine::new(LayoutKind::Spatial2D),
        Rect::new(0, 0, 300, 200),
    );
    assert!(world.geometry(view(1)).is_some());
    assert!(world.geometry(view(2)).is_some());
    assert_eq!(world.tiled_ancestor(view(2)), Some(view(1)));
}
