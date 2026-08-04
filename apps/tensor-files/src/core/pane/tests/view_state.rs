use super::*;

#[test]
fn compact_view_scroll_is_pane_local_and_clamped() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let first = controller.focused().unwrap();
    let second = controller.split(first).unwrap();

    assert_eq!(
        controller.scroll_view(first, 120.0, 30.0, 200.0, 40.0),
        Some(ViewState {
            scroll_x: 120.0,
            scroll_y: 30.0,
            max_scroll_x: 200.0,
            max_scroll_y: 40.0,
            ..ViewState::default()
        })
    );
    assert_eq!(
        controller.scroll_view(first, 500.0, 500.0, 200.0, 40.0),
        Some(ViewState {
            scroll_x: 200.0,
            scroll_y: 40.0,
            max_scroll_x: 200.0,
            max_scroll_y: 40.0,
            ..ViewState::default()
        })
    );
    assert_eq!(
        controller.scroll_view(first, -300.0, -100.0, 200.0, 40.0),
        Some(ViewState {
            scroll_x: 0.0,
            scroll_y: 0.0,
            max_scroll_x: 200.0,
            max_scroll_y: 40.0,
            ..ViewState::default()
        })
    );

    assert_eq!(controller.pane(second).unwrap().view.scroll_x, 0.0);
    assert_eq!(controller.pane(second).unwrap().view.scroll_y, 0.0);
}

#[test]
fn compact_view_absolute_scroll_is_pane_local_and_clamped() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let first = controller.focused().unwrap();
    let second = controller.split(first).unwrap();

    assert_eq!(
        controller.set_view_scroll(first, 260.0, 90.0, 200.0, 40.0),
        Some(ViewState {
            scroll_x: 200.0,
            scroll_y: 40.0,
            max_scroll_x: 200.0,
            max_scroll_y: 40.0,
            ..ViewState::default()
        })
    );
    assert_eq!(
        controller.set_view_scroll(first, -20.0, -10.0, 200.0, 40.0),
        Some(ViewState {
            scroll_x: 0.0,
            scroll_y: 0.0,
            max_scroll_x: 200.0,
            max_scroll_y: 40.0,
            ..ViewState::default()
        })
    );

    assert_eq!(controller.pane(second).unwrap().view.scroll_x, 0.0);
    assert_eq!(controller.pane(second).unwrap().view.scroll_y, 0.0);
}

#[test]
fn viewport_bounds_never_exceed_measured_pane_extent() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let pane_id = controller.focused().unwrap();

    assert_eq!(
        controller.set_viewport_bounds(pane_id, 320.9, 119.7, 1_000.0, 500.0),
        Some(true)
    );

    let view = &controller.pane(pane_id).unwrap().view;
    assert_eq!(view.viewport_width, 320.0);
    assert_eq!(view.viewport_height, 119.0);
}

#[test]
fn navigation_resets_scroll_but_reload_preserves_it() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let pane_id = controller.focused().unwrap();

    controller.set_view_scroll(pane_id, 120.0, 30.0, 200.0, 40.0);
    controller.reload(pane_id).unwrap();
    assert_eq!(controller.pane(pane_id).unwrap().view.scroll_x, 120.0);
    assert_eq!(controller.pane(pane_id).unwrap().view.scroll_y, 30.0);

    controller.load(pane_id, PathBuf::from("/tmp/b")).unwrap();
    assert_eq!(controller.pane(pane_id).unwrap().view.scroll_x, 0.0);
    assert_eq!(controller.pane(pane_id).unwrap().view.scroll_y, 0.0);

    controller.set_view_scroll(pane_id, 80.0, 20.0, 200.0, 40.0);
    controller.go_back(pane_id).unwrap();
    assert_eq!(controller.pane(pane_id).unwrap().view.scroll_x, 0.0);
    assert_eq!(controller.pane(pane_id).unwrap().view.scroll_y, 0.0);

    controller.set_view_scroll(pane_id, 80.0, 20.0, 200.0, 40.0);
    controller.go_forward(pane_id).unwrap();
    assert_eq!(controller.pane(pane_id).unwrap().view.scroll_x, 0.0);
    assert_eq!(controller.pane(pane_id).unwrap().view.scroll_y, 0.0);
}

#[test]
fn sort_role_uses_file_manager_default_order_and_remembers_per_role_order() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let pane_id = controller.focused().unwrap();

    assert_eq!(
        controller.preferred_sort_order(pane_id, SortRole::Name),
        Some(SortOrder::Ascending)
    );
    assert_eq!(
        controller.preferred_sort_order(pane_id, SortRole::Size),
        Some(SortOrder::Descending)
    );

    let (size_sort, _) = controller
        .set_sort_role(pane_id, SortRole::Size)
        .expect("pane exists");
    assert_eq!(
        size_sort,
        SortDescriptor {
            role: SortRole::Size,
            order: SortOrder::Descending,
            ..SortDescriptor::default()
        }
    );

    controller
        .set_sort_order(pane_id, SortOrder::Ascending)
        .expect("pane exists");
    assert_eq!(
        controller.preferred_sort_order(pane_id, SortRole::Size),
        Some(SortOrder::Ascending)
    );

    let (name_sort, _) = controller
        .set_sort_role(pane_id, SortRole::Name)
        .expect("pane exists");
    assert_eq!(
        name_sort,
        SortDescriptor {
            role: SortRole::Name,
            order: SortOrder::Ascending,
            ..SortDescriptor::default()
        }
    );

    controller
        .set_sort_order(pane_id, SortOrder::Descending)
        .expect("pane exists");
    let (size_sort, _) = controller
        .set_sort_role(pane_id, SortRole::Size)
        .expect("pane exists");
    assert_eq!(
        size_sort,
        SortDescriptor {
            role: SortRole::Size,
            order: SortOrder::Ascending,
            ..SortDescriptor::default()
        }
    );
    assert_eq!(
        controller.preferred_sort_order(pane_id, SortRole::Name),
        Some(SortOrder::Descending)
    );
}

#[test]
fn split_inherits_sort_order_preferences_but_updates_are_pane_local() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let first = controller.focused().unwrap();

    controller
        .set_sort_role(first, SortRole::Size)
        .expect("pane exists");
    controller
        .set_sort_order(first, SortOrder::Ascending)
        .expect("pane exists");

    let second = controller.split(first).unwrap();
    assert_eq!(
        controller.preferred_sort_order(second, SortRole::Size),
        Some(SortOrder::Ascending)
    );

    controller
        .set_sort_order(first, SortOrder::Descending)
        .expect("pane exists");

    assert_eq!(
        controller.preferred_sort_order(first, SortRole::Size),
        Some(SortOrder::Descending)
    );
    assert_eq!(
        controller.preferred_sort_order(second, SortRole::Size),
        Some(SortOrder::Ascending)
    );
}

#[test]
fn sort_folder_and_hidden_toggles_are_pane_local_after_split() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let first = controller.focused().unwrap();

    let second = controller.split(first).unwrap();

    let (first_sort, _) = controller
        .set_sort_folders_first(first, false)
        .expect("pane exists");
    assert!(!first_sort.folders_first);
    assert!(
        controller
            .sort_descriptor(second)
            .expect("pane exists")
            .folders_first
    );

    let (second_sort, _) = controller
        .set_sort_hidden_last(second, true)
        .expect("pane exists");
    assert!(second_sort.hidden_last);
    assert!(
        !controller
            .sort_descriptor(first)
            .expect("pane exists")
            .hidden_last
    );
}

#[test]
fn zoom_level_maps_to_icon_size_and_clamps() {
    assert_eq!(icon_size_for_zoom_level(MIN_ZOOM_LEVEL - 1), 16.0);
    assert_eq!(icon_size_for_zoom_level(0), 16.0);
    assert_eq!(icon_size_for_zoom_level(1), 22.0);
    assert_eq!(icon_size_for_zoom_level(2), 32.0);
    assert_eq!(icon_size_for_zoom_level(DEFAULT_ZOOM_LEVEL), 48.0);
    assert_eq!(icon_size_for_zoom_level(4), 64.0);
    assert_eq!(icon_size_for_zoom_level(MAX_ZOOM_LEVEL), 256.0);
    assert_eq!(icon_size_for_zoom_level(MAX_ZOOM_LEVEL + 1), 256.0);
}

#[test]
fn zoom_level_is_pane_local_and_split_inherits_source_view() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let first = controller.focused().unwrap();

    let zoomed = controller
        .apply_zoom_change(first, ZoomChange::In)
        .expect("pane exists");
    assert_eq!(zoomed.zoom_level, DEFAULT_ZOOM_LEVEL + 1);
    assert_eq!(zoomed.icon_size(), 64.0);

    let second = controller.split(first).unwrap();
    assert_eq!(
        controller.pane(second).unwrap().view.zoom_level,
        DEFAULT_ZOOM_LEVEL + 1
    );

    let first_view = controller
        .set_zoom_level(first, MAX_ZOOM_LEVEL + 10)
        .expect("pane exists");
    assert_eq!(first_view.zoom_level, MAX_ZOOM_LEVEL);
    assert_eq!(first_view.icon_size(), 256.0);

    let second_view = controller
        .set_zoom_level(second, MIN_ZOOM_LEVEL - 10)
        .expect("pane exists");
    assert_eq!(second_view.zoom_level, MIN_ZOOM_LEVEL);
    assert_eq!(second_view.icon_size(), 16.0);
    assert_eq!(
        controller.pane(first).unwrap().view.zoom_level,
        MAX_ZOOM_LEVEL
    );

    let reset = controller
        .apply_zoom_change(second, ZoomChange::Reset)
        .expect("pane exists");
    assert_eq!(reset.zoom_level, DEFAULT_ZOOM_LEVEL);
}

#[test]
fn view_mode_is_pane_local_resets_scroll_and_split_inherits_source_view() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let first = controller.focused().unwrap();
    controller
        .set_view_scroll(first, 120.0, 30.0, 200.0, 100.0)
        .unwrap();

    let icons = controller
        .set_view_mode(first, ViewMode::Icons)
        .expect("pane exists");
    assert_eq!(icons.view_mode, ViewMode::Icons);
    assert_eq!(icons.scroll_x, 0.0);
    assert_eq!(icons.scroll_y, 0.0);

    let second = controller.split(first).unwrap();
    assert_eq!(
        controller.pane(second).unwrap().view.view_mode,
        ViewMode::Icons
    );

    controller
        .set_view_mode(second, ViewMode::Details)
        .expect("pane exists");
    assert_eq!(
        controller.pane(first).unwrap().view.view_mode,
        ViewMode::Icons
    );
    assert_eq!(
        controller.pane(second).unwrap().view.view_mode,
        ViewMode::Details
    );
}
