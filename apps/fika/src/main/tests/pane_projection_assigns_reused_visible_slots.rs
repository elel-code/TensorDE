#[test]
fn pane_projection_assigns_reused_visible_slots() {
    let mut scene = test_scene(
        (0..80)
            .map(|index| test_entry(&format!("entry-{index:02}.txt"), false))
            .collect(),
        ShellViewMode::Icons,
    );
    let size = PhysicalSize::new(420, 260);

    let initial_stats = scene.update_visible_slot_pools(size);
    assert!(initial_stats.active > 0);
    assert_eq!(initial_stats.allocated, initial_stats.active);
    let initial_projection = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();
    let retained_slot = initial_projection.visible_items[0].slot_id;
    assert_ne!(retained_slot, 0);
    assert!(
        initial_projection
            .visible_items
            .iter()
            .all(|item| item.slot_id != 0)
    );

    let next_stats = scene.update_visible_slot_pools(size);
    assert_eq!(next_stats.reused, next_stats.active);
    let next_projection = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();
    assert_eq!(next_projection.visible_items[0].slot_id, retained_slot);
}

#[test]
fn prepared_pane_projections_match_direct_projection() {
    let mut scene = test_scene(
        (0..60)
            .map(|index| test_entry(&format!("entry-{index:02}.txt"), index % 4 == 0))
            .collect(),
        ShellViewMode::Icons,
    );
    let size = PhysicalSize::new(700, 320);

    let mut layouts = scene.prepare_frame_projection_layouts(size);
    scene.update_visible_slot_pools_for_projection_layouts(&mut layouts);
    assert!(
        layouts
            .layouts
            .iter()
            .flat_map(|projection| &projection.visible_items)
            .all(|item| item.path.is_none())
    );
    let frame_projections = scene.pane_projections_from_layouts(layouts);
    let prepared = frame_projections
        .projections()
        .iter()
        .find(|projection| projection.geometry.kind == ShellPaneId::SLOT_0)
        .unwrap();
    let direct = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();

    assert_eq!(prepared.geometry, direct.geometry);
    assert_eq!(prepared.scroll_metrics, direct.scroll_metrics);
    assert_eq!(prepared.visible_items, direct.visible_items);
    assert!(prepared.visible_items.iter().all(|item| item.slot_id != 0));
    assert_eq!(prepared.view.path, direct.view.path);
    assert_eq!(prepared.view.view_mode, direct.view.view_mode);
}
