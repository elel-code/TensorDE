#[test]
fn pending_reflow_does_not_alias_recycled_visible_entity() {
    let mut scene = test_scene(
        (0..8)
            .map(|index| test_entry(&format!("item-{index:02}.txt"), false))
            .collect(),
        ShellViewMode::Icons,
    );
    let narrow = PhysicalSize::new(520, 360);
    let wide = PhysicalSize::new(800, 360);
    let target = PathBuf::from("/tmp/item-02.txt");

    scene.update_visible_slot_pools(narrow);
    let original_entity = scene
        .visible_slots
        .get(ShellPaneId::SLOT_0)
        .entity_for_path(&target)
        .expect("warm item has a retained entity");
    assert!(scene.reflow_pane_items_after_window_resize(narrow, wide));

    scene.visible_slots.clear(ShellPaneId::SLOT_0);
    scene.update_visible_slot_pools(wide);
    let recycled_entity = scene
        .visible_slots
        .get(ShellPaneId::SLOT_0)
        .entity_for_path(&target)
        .expect("item is visible again with a fresh entity");
    assert_ne!(original_entity, recycled_entity);

    let projection = scene.pane_projection(ShellPaneId::SLOT_0, wide).unwrap();
    let target_item = projection
        .visible_items
        .iter()
        .find(|item| {
            item.entry_index
                .and_then(|index| projection.view.entries.get(index))
                .is_some_and(|entry| entry.name.as_ref() == "item-02.txt")
        })
        .expect("target remains visible");
    assert_eq!(target_item.reflow_offset, (0.0, 0.0));

    assert!(!ui::item_reflow::start_due_item_reflow_transitions(
        &mut scene,
        Instant::now() + ITEM_REFLOW_ANIMATION_DELAY + Duration::from_millis(1),
    ));
    assert!(
        scene
            .animations
            .item_reflow_transition_for_entity(ShellPaneId::SLOT_0, recycled_entity)
            .is_none()
    );
}

#[test]
fn cold_pending_reflow_keeps_path_fallback_after_slot_population() {
    let mut scene = test_scene(
        (0..8)
            .map(|index| test_entry(&format!("item-{index:02}.txt"), false))
            .collect(),
        ShellViewMode::Icons,
    );
    let narrow = PhysicalSize::new(520, 360);
    let wide = PhysicalSize::new(800, 360);
    let target = PathBuf::from("/tmp/item-02.txt");

    let previous = scene
        .visible_item_reflow_rects_for_pane(ShellPaneId::SLOT_0, narrow)
        .rect_for(None, &target)
        .expect("cold geometry uses the path fallback");
    let next = scene
        .visible_item_reflow_rects_for_pane(ShellPaneId::SLOT_0, wide)
        .rect_for(None, &target)
        .expect("cold next geometry uses the path fallback");
    assert!(scene.reflow_pane_items_after_window_resize(narrow, wide));

    scene.update_visible_slot_pools(wide);
    let projection = scene.pane_projection(ShellPaneId::SLOT_0, wide).unwrap();
    let target_item = projection
        .visible_items
        .iter()
        .find(|item| {
            item.entry_index
                .and_then(|index| projection.view.entries.get(index))
                .is_some_and(|entry| entry.name.as_ref() == "item-02.txt")
        })
        .expect("target remains visible after slot population");
    assert_eq!(
        target_item.reflow_offset,
        (previous.x - next.x, previous.y - next.y)
    );
}

#[test]
fn navigation_and_split_topology_cancel_pending_reflow() {
    let root = test_dir("pending-reflow-topology");
    let split = root.join("split");
    fs::create_dir_all(&split).unwrap();
    for index in 0..8 {
        fs::write(root.join(format!("item-{index:02}.txt")), b"item").unwrap();
    }

    let narrow = PhysicalSize::new(520, 360);
    let wide = PhysicalSize::new(800, 360);
    let mut scene = ShellScene::load(root.clone(), ShellViewMode::Icons).unwrap();
    scene.update_visible_slot_pools(narrow);
    assert!(scene.reflow_pane_items_after_window_resize(narrow, wide));
    assert!(ui::item_reflow::has_pending_item_reflow(&scene));
    assert!(scene.begin_pane_navigation(ShellPaneId::SLOT_0, split.clone(), wide));
    assert!(!ui::item_reflow::has_pending_item_reflow(&scene));

    assert!(scene.cancel_pane_navigation(ShellPaneId::SLOT_0));
    assert!(scene.reflow_pane_items_after_window_resize(narrow, wide));
    assert!(scene.open_split_pane(split, wide).unwrap());
    assert!(!ui::item_reflow::has_pending_item_reflow(&scene));
    assert_eq!(scene.animations.item_reflow_transition_count(), 0);

    fs::remove_dir_all(root).unwrap();
}

#[test]
fn projection_samples_time_only_while_reflow_state_exists() {
    let mut scene = test_scene(
        (0..8)
            .map(|index| test_entry(&format!("item-{index:02}.txt"), false))
            .collect(),
        ShellViewMode::Icons,
    );
    let narrow = PhysicalSize::new(520, 360);
    let wide = PhysicalSize::new(800, 360);

    assert_eq!(scene.projection_reflow_time(ShellPaneId::ALL), None);
    scene.update_visible_slot_pools(narrow);
    assert!(scene.reflow_pane_items_after_window_resize(narrow, wide));
    assert!(scene.projection_reflow_time(ShellPaneId::ALL).is_some());

    assert!(ui::item_reflow::start_due_item_reflow_transitions(
        &mut scene,
        Instant::now() + ITEM_REFLOW_ANIMATION_DELAY + Duration::from_millis(1),
    ));
    assert!(!ui::item_reflow::has_pending_item_reflow(&scene));
    assert!(scene.projection_reflow_time(ShellPaneId::ALL).is_some());

    scene.animations.clear();
    assert_eq!(scene.projection_reflow_time(ShellPaneId::ALL), None);
}
