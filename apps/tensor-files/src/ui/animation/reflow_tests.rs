use super::*;

#[test]
fn item_reflow_lookup_is_pane_scoped_and_replaces_only_target_pane() {
    let mut runtime = ShellAnimationRuntime::default();
    let shared_path = PathBuf::from("/tmp/shared-item");
    let replacement_path = PathBuf::from("/tmp/replacement-item");

    assert!(runtime.start_item_reflow(
        ShellPaneId::SLOT_0,
        HashMap::from([(shared_path.clone(), rect(0.0, 0.0))]),
        HashMap::from([(shared_path.clone(), rect(12.0, 0.0))]),
    ));
    assert!(runtime.start_item_reflow(
        ShellPaneId::SLOT_1,
        HashMap::from([(shared_path.clone(), rect(0.0, 0.0))]),
        HashMap::from([(shared_path.clone(), rect(0.0, 18.0))]),
    ));
    let now = Instant::now();
    assert!(
        runtime
            .item_reflow_offset_for_path_at(ShellPaneId::SLOT_0, &shared_path, now)
            .is_some()
    );
    assert!(
        runtime
            .item_reflow_offset_for_path_at(ShellPaneId::SLOT_1, &shared_path, now)
            .is_some()
    );
    assert_eq!(runtime.item_reflow_transition_count(), 2);

    assert!(runtime.start_item_reflow(
        ShellPaneId::SLOT_0,
        HashMap::from([(replacement_path.clone(), rect(0.0, 0.0))]),
        HashMap::from([(replacement_path.clone(), rect(24.0, 0.0))]),
    ));
    let now = Instant::now();
    assert!(
        runtime
            .item_reflow_offset_for_path_at(ShellPaneId::SLOT_0, &shared_path, now)
            .is_none()
    );
    assert!(
        runtime
            .item_reflow_offset_for_path_at(ShellPaneId::SLOT_0, &replacement_path, now)
            .is_some()
    );
    assert!(
        runtime
            .item_reflow_offset_for_path_at(ShellPaneId::SLOT_1, &shared_path, now)
            .is_some()
    );
    assert_eq!(runtime.item_reflow_transition_count(), 2);

    runtime.item_reflow_transitions[ShellPaneId::SLOT_0.index()].started =
        Some(Instant::now() - ITEM_REFLOW_ANIMATION_DURATION);
    assert!(runtime.prune_finished());
    assert!(
        runtime
            .item_reflow_transition(ShellPaneId::SLOT_0, &replacement_path)
            .is_none()
    );
    assert!(
        runtime
            .item_reflow_transition(ShellPaneId::SLOT_1, &shared_path)
            .is_some()
    );
    assert_eq!(runtime.item_reflow_transition_count(), 1);
}

#[test]
fn entity_keyed_reflow_does_not_alias_a_recycled_widget_identity() {
    let mut world = bevy_ecs::world::World::new();
    let retained_entity = world.spawn_empty().id();
    let recycled_entity = world.spawn_empty().id();
    let path = PathBuf::from("/tmp/entity-keyed-item");
    let mut runtime = ShellAnimationRuntime::default();

    assert!(runtime.start_item_reflow_with_entity_lookup(
        ShellPaneId::SLOT_0,
        HashMap::from([(path.clone(), rect(0.0, 0.0))]),
        HashMap::from([(path.clone(), rect(24.0, 0.0))]),
        |_| Some(retained_entity),
    ));
    let now = Instant::now();

    assert!(
        runtime
            .item_reflow_transition_for_entity(ShellPaneId::SLOT_0, retained_entity)
            .is_some()
    );
    assert!(
        runtime
            .item_reflow_offset_for_entity_at(ShellPaneId::SLOT_0, retained_entity, now)
            .is_some()
    );
    assert!(
        runtime
            .item_reflow_offset_for_entity_at(ShellPaneId::SLOT_0, recycled_entity, now)
            .is_none()
    );
    assert!(
        runtime
            .item_reflow_offset_for_path_at(ShellPaneId::SLOT_0, &path, now)
            .is_none()
    );
}

fn rect(x: f32, y: f32) -> ViewRect {
    ViewRect {
        x,
        y,
        width: 10.0,
        height: 10.0,
    }
}
