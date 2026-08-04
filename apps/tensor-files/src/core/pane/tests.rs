use super::super::directory::DirectoryListerEvent;
use super::super::entries::{Entry, EntryData};
use super::*;
use std::sync::Arc;

#[test]
fn split_allocates_distinct_pane_identity_for_same_path() {
    let mut controller = PaneController::new(PathBuf::from("/tmp"));
    let first = controller.focused().unwrap();
    let second = controller.split(first).unwrap();

    assert_ne!(first, second);
    assert_eq!(
        controller.pane(first).unwrap().current_dir,
        PathBuf::from("/tmp")
    );
    assert_eq!(
        controller.pane(second).unwrap().current_dir,
        PathBuf::from("/tmp")
    );
    assert_eq!(controller.focused(), Some(second));
}

#[test]
fn stale_result_for_closed_pane_is_ignored() {
    let mut controller = PaneController::new(PathBuf::from("/tmp"));
    let first = controller.focused().unwrap();
    let second = controller.split(first).unwrap();
    let event = controller.reload(second).unwrap();

    assert!(controller.close(second));
    assert!(controller.apply_lister_event(event).is_none());
}

#[test]
fn stale_generation_result_is_ignored() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let pane_id = controller.focused().unwrap();
    controller.load(pane_id, PathBuf::from("/tmp/b"));

    let event = DirectoryListerEvent::ListingRefreshed {
        pane_id,
        generation: Generation(0),
        request_serial: RequestSerial(1),
        path: PathBuf::from("/tmp/b"),
        entries: Arc::new(vec![test_entry_at("/tmp/b", "stale.txt")]),
    };

    assert!(controller.apply_lister_event(event).is_none());
    assert!(controller.pane(pane_id).unwrap().model.is_empty());
}

#[test]
fn same_path_split_panes_apply_their_own_lister_events() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let first = controller.focused().unwrap();
    let second = controller.split(first).unwrap();
    let path = PathBuf::from("/tmp/a/new.txt");

    controller.apply_lister_event(DirectoryListerEvent::ItemsAdded {
        pane_id: first,
        generation: controller.pane(first).unwrap().generation,
        request_serial: RequestSerial(1),
        path: PathBuf::from("/tmp/a"),
        entries: vec![test_entry("new.txt")],
    });

    assert_eq!(
        controller.pane(first).unwrap().model.index_of_path(&path),
        Some(0)
    );
    assert!(controller.pane(second).unwrap().model.is_empty());

    controller.apply_lister_event(DirectoryListerEvent::ItemsAdded {
        pane_id: second,
        generation: controller.pane(second).unwrap().generation,
        request_serial: RequestSerial(1),
        path: PathBuf::from("/tmp/a"),
        entries: vec![test_entry("new.txt")],
    });

    assert_eq!(
        controller.pane(first).unwrap().model.index_of_path(&path),
        Some(0)
    );
    assert_eq!(
        controller.pane(second).unwrap().model.index_of_path(&path),
        Some(0)
    );
}

#[test]
fn manual_refresh_on_inactive_pane_targets_inactive_pane() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let first = controller.focused().unwrap();
    let second = controller.split(first).unwrap();
    controller.load(second, PathBuf::from("/tmp/b"));
    controller.focus(second);

    let event = controller.reload(first).unwrap();

    assert_eq!(event.pane_id(), first);
    assert_eq!(event.path(), Path::new("/tmp/a"));
    assert_eq!(controller.focused(), Some(second));
}

#[test]
fn focus_never_retargets_async_result() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let first = controller.focused().unwrap();
    let second = controller.split(first).unwrap();
    controller.load(second, PathBuf::from("/tmp/b"));
    controller.focus(first);
    let event = DirectoryListerEvent::ListingCompleted {
        pane_id: second,
        generation: controller.pane(second).unwrap().generation,
        request_serial: RequestSerial(1),
        path: PathBuf::from("/tmp/b"),
    };

    assert!(controller.apply_lister_event(event).is_some());
    assert_eq!(controller.focused(), Some(first));
}

#[test]
fn loading_started_keeps_previous_model_until_listing_refresh() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let pane_id = controller.focused().unwrap();
    let generation = controller.pane(pane_id).unwrap().generation;
    controller.apply_lister_event(DirectoryListerEvent::ListingRefreshed {
        pane_id,
        generation,
        request_serial: RequestSerial(1),
        path: PathBuf::from("/tmp/a"),
        entries: Arc::new(vec![test_entry_at("/tmp/a", "old.txt")]),
    });

    let started = controller.load(pane_id, PathBuf::from("/tmp/b")).unwrap();
    let signals = controller.apply_lister_event(started.clone()).unwrap();

    assert!(signals.is_empty());
    let pane = controller.pane(pane_id).unwrap();
    assert_eq!(pane.current_dir, PathBuf::from("/tmp/b"));
    assert_eq!(pane.model.directory(), Path::new("/tmp/a"));
    assert_eq!(
        pane.model.path_for_index(0),
        Some(PathBuf::from("/tmp/a/old.txt"))
    );

    let signals = controller
        .apply_lister_event(DirectoryListerEvent::ListingRefreshed {
            pane_id,
            generation: started.generation(),
            request_serial: started.request_serial(),
            path: PathBuf::from("/tmp/b"),
            entries: Arc::new(vec![test_entry_at("/tmp/b", "new.txt")]),
        })
        .unwrap();

    assert_eq!(signals, vec![DirectoryModelSignal::ModelReset]);
    let pane = controller.pane(pane_id).unwrap();
    assert_eq!(pane.model.directory(), Path::new("/tmp/b"));
    assert_eq!(
        pane.model.path_for_index(0),
        Some(PathBuf::from("/tmp/b/new.txt"))
    );
}

#[test]
fn history_navigation_is_pane_scoped() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let first = controller.focused().unwrap();
    let second = controller.split(first).unwrap();
    controller.load(first, PathBuf::from("/tmp/a1"));
    controller.load(second, PathBuf::from("/tmp/b1"));
    controller.focus(second);

    let event = controller.go_back(first).unwrap();

    assert_eq!(event.pane_id(), first);
    assert_eq!(
        controller.pane(first).unwrap().current_dir,
        PathBuf::from("/tmp/a")
    );
    assert_eq!(
        controller.pane(second).unwrap().current_dir,
        PathBuf::from("/tmp/b1")
    );
    assert_eq!(controller.focused(), Some(second));
    assert!(controller.can_go_forward(first));
}

#[test]
fn forward_navigation_uses_the_same_pane_history() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let pane_id = controller.focused().unwrap();
    controller.load(pane_id, PathBuf::from("/tmp/b"));
    controller.go_back(pane_id);

    let event = controller.go_forward(pane_id).unwrap();

    assert_eq!(event.pane_id(), pane_id);
    assert_eq!(
        controller.pane(pane_id).unwrap().current_dir,
        PathBuf::from("/tmp/b")
    );
    assert!(!controller.can_go_forward(pane_id));
}

#[test]
fn selection_is_scoped_to_pane_identity() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let first = controller.focused().unwrap();
    let second = controller.split(first).unwrap();
    let path = PathBuf::from("/tmp/a/file.txt");

    controller.pane_mut(first).unwrap().model.replace_listing(
        PathBuf::from("/tmp/a"),
        listing(vec![test_entry_with_path(path.clone())]),
    );
    controller.pane_mut(second).unwrap().model.replace_listing(
        PathBuf::from("/tmp/a"),
        listing(vec![test_entry_with_path(path.clone())]),
    );

    assert!(controller.select_only(first, path.clone()));

    assert!(controller.is_selected(first, &path));
    assert!(!controller.is_selected(second, &path));
}

#[test]
fn selecting_already_single_selected_item_is_noop() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let pane_id = controller.focused().unwrap();
    let path = PathBuf::from("/tmp/a/file.txt");
    controller.pane_mut(pane_id).unwrap().model.replace_listing(
        PathBuf::from("/tmp/a"),
        listing(vec![test_entry_with_path(path.clone())]),
    );

    assert!(controller.select_only(pane_id, path.clone()));
    let revision = controller.pane(pane_id).unwrap().selection.revision();
    assert!(!controller.select_only(pane_id, path));
    assert_eq!(
        controller.pane(pane_id).unwrap().selection.revision(),
        revision
    );
}

#[test]
fn split_panes_do_not_share_mutable_model_entries() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let first = controller.focused().unwrap();
    let path = PathBuf::from("/tmp/a/file.txt");
    controller.pane_mut(first).unwrap().model.replace_listing(
        PathBuf::from("/tmp/a"),
        listing(vec![test_entry_with_path(path.clone())]),
    );
    let second = controller.split(first).unwrap();
    let generation = controller.pane(first).unwrap().generation;

    controller.apply_lister_event(DirectoryListerEvent::ItemsDeleted {
        pane_id: first,
        generation,
        request_serial: RequestSerial(1),
        path: PathBuf::from("/tmp/a"),
        paths: vec![path.clone()],
    });

    assert!(controller.pane(first).unwrap().model.is_empty());
    assert_eq!(
        controller.pane(second).unwrap().model.index_of_path(&path),
        Some(0)
    );
}

#[test]
fn selection_is_pruned_after_model_change() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let pane_id = controller.focused().unwrap();
    let keep = PathBuf::from("/tmp/a/keep.txt");
    let remove = PathBuf::from("/tmp/a/remove.txt");
    let generation = controller.pane(pane_id).unwrap().generation;

    controller.pane_mut(pane_id).unwrap().model.replace_listing(
        PathBuf::from("/tmp/a"),
        listing(vec![
            test_entry_with_path(keep.clone()),
            test_entry_with_path(remove.clone()),
        ]),
    );
    controller.select_all(pane_id);

    controller.apply_lister_event(DirectoryListerEvent::ItemsDeleted {
        pane_id,
        generation,
        request_serial: RequestSerial(1),
        path: PathBuf::from("/tmp/a"),
        paths: vec![remove.clone()],
    });

    assert_eq!(controller.selected_paths(pane_id), Some(vec![keep]));
}

#[test]
fn select_all_keeps_selection_compact_and_toggle_excludes_item() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let pane_id = controller.focused().unwrap();
    controller.pane_mut(pane_id).unwrap().model.replace_listing(
        PathBuf::from("/tmp/a"),
        listing(vec![
            test_entry("a.txt"),
            test_entry("b.txt"),
            test_entry("c.txt"),
        ]),
    );

    assert_eq!(controller.select_all(pane_id), Some(3));
    let selection = &controller.pane(pane_id).unwrap().selection;
    assert!(selection.is_all_selected());
    assert!(selection.selected_ids().is_empty());
    assert_eq!(controller.selected_count(pane_id), Some(3));

    assert_eq!(
        controller.toggle_selection(pane_id, PathBuf::from("/tmp/a/b.txt")),
        Some(false)
    );
    assert_eq!(controller.selected_count(pane_id), Some(2));
    assert!(!controller.is_selected(pane_id, Path::new("/tmp/a/b.txt")));
    assert_eq!(
        controller.selected_paths(pane_id),
        Some(vec![
            PathBuf::from("/tmp/a/a.txt"),
            PathBuf::from("/tmp/a/c.txt")
        ])
    );

    assert_eq!(
        controller.toggle_selection(pane_id, PathBuf::from("/tmp/a/b.txt")),
        Some(true)
    );
    assert_eq!(controller.selected_count(pane_id), Some(3));
}

#[test]
fn selection_tracks_item_identity_across_rename_refresh() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let pane_id = controller.focused().unwrap();
    let generation = controller.pane(pane_id).unwrap().generation;
    let old_path = PathBuf::from("/tmp/a/old.txt");
    let new_path = PathBuf::from("/tmp/a/new.txt");

    controller.pane_mut(pane_id).unwrap().model.replace_listing(
        PathBuf::from("/tmp/a"),
        listing(vec![test_entry("old.txt")]),
    );
    assert!(controller.select_only(pane_id, old_path.clone()));

    controller.apply_lister_event(DirectoryListerEvent::ItemsRefreshed {
        pane_id,
        generation,
        request_serial: RequestSerial(1),
        path: PathBuf::from("/tmp/a"),
        pairs: vec![super::super::directory::RefreshPair {
            old_path,
            entry: Some(test_entry("new.txt")),
        }],
    });

    assert_eq!(
        controller.selected_paths(pane_id),
        Some(vec![new_path.clone()])
    );
    assert!(controller.is_selected(pane_id, &new_path));
}

#[test]
fn range_selection_uses_model_order_and_keeps_anchor() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let pane_id = controller.focused().unwrap();
    controller.pane_mut(pane_id).unwrap().model.replace_listing(
        PathBuf::from("/tmp/a"),
        listing(vec![
            test_entry("a.txt"),
            test_entry("b.txt"),
            test_entry("c.txt"),
            test_entry("d.txt"),
        ]),
    );

    assert!(controller.select_only(pane_id, PathBuf::from("/tmp/a/b.txt")));
    assert_eq!(
        controller.select_range_to(pane_id, PathBuf::from("/tmp/a/d.txt")),
        Some(3)
    );

    assert_eq!(
        controller.selected_paths(pane_id),
        Some(vec![
            PathBuf::from("/tmp/a/b.txt"),
            PathBuf::from("/tmp/a/c.txt"),
            PathBuf::from("/tmp/a/d.txt")
        ])
    );
    assert_eq!(
        controller.selection_anchor_path(pane_id),
        Some(PathBuf::from("/tmp/a/b.txt"))
    );
}

#[test]
fn range_selection_without_anchor_starts_at_target() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let pane_id = controller.focused().unwrap();
    controller.pane_mut(pane_id).unwrap().model.replace_listing(
        PathBuf::from("/tmp/a"),
        listing(vec![test_entry("a.txt"), test_entry("b.txt")]),
    );

    assert_eq!(
        controller.select_range_to(pane_id, PathBuf::from("/tmp/a/b.txt")),
        Some(1)
    );

    assert_eq!(
        controller.selected_paths(pane_id),
        Some(vec![PathBuf::from("/tmp/a/b.txt")])
    );
}

#[test]
fn keyboard_selection_moves_by_model_order() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let pane_id = controller.focused().unwrap();
    controller.pane_mut(pane_id).unwrap().model.replace_listing(
        PathBuf::from("/tmp/a"),
        listing(vec![test_entry("a.txt"), test_entry("b.txt")]),
    );

    assert_eq!(
        controller.move_selection(pane_id, SelectionMove::Next, false),
        Some(1)
    );
    assert_eq!(
        controller.selected_paths(pane_id),
        Some(vec![PathBuf::from("/tmp/a/a.txt")])
    );

    assert_eq!(
        controller.move_selection(pane_id, SelectionMove::Next, false),
        Some(1)
    );
    assert_eq!(
        controller.selected_paths(pane_id),
        Some(vec![PathBuf::from("/tmp/a/b.txt")])
    );

    assert_eq!(
        controller.move_selection(pane_id, SelectionMove::Previous, false),
        Some(1)
    );
    assert_eq!(
        controller.selected_paths(pane_id),
        Some(vec![PathBuf::from("/tmp/a/a.txt")])
    );
}

#[test]
fn keyboard_range_selection_keeps_anchor_and_moves_active_path() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let pane_id = controller.focused().unwrap();
    controller.pane_mut(pane_id).unwrap().model.replace_listing(
        PathBuf::from("/tmp/a"),
        listing(vec![
            test_entry("a.txt"),
            test_entry("b.txt"),
            test_entry("c.txt"),
        ]),
    );

    assert!(controller.select_only(pane_id, PathBuf::from("/tmp/a/a.txt")));
    assert_eq!(
        controller.move_selection(pane_id, SelectionMove::Next, true),
        Some(2)
    );
    assert_eq!(
        controller.move_selection(pane_id, SelectionMove::Next, true),
        Some(3)
    );

    assert_eq!(
        controller.selected_paths(pane_id),
        Some(vec![
            PathBuf::from("/tmp/a/a.txt"),
            PathBuf::from("/tmp/a/b.txt"),
            PathBuf::from("/tmp/a/c.txt")
        ])
    );
    assert_eq!(
        controller.selection_anchor_path(pane_id),
        Some(PathBuf::from("/tmp/a/a.txt"))
    );
    assert_eq!(
        controller.selection_active_path(pane_id),
        Some(PathBuf::from("/tmp/a/c.txt"))
    );

    assert_eq!(
        controller.move_selection(pane_id, SelectionMove::Previous, true),
        Some(2)
    );
    assert_eq!(
        controller.selected_paths(pane_id),
        Some(vec![
            PathBuf::from("/tmp/a/a.txt"),
            PathBuf::from("/tmp/a/b.txt")
        ])
    );
}

#[test]
fn rubber_band_selection_replaces_paths_by_model_indexes() {
    let mut controller = PaneController::new(PathBuf::from("/tmp/a"));
    let pane_id = controller.focused().unwrap();
    controller.pane_mut(pane_id).unwrap().model.replace_listing(
        PathBuf::from("/tmp/a"),
        listing(vec![
            test_entry("a.txt"),
            test_entry("b.txt"),
            test_entry("c.txt"),
        ]),
    );

    assert_eq!(
        controller.replace_selection_by_indexes(pane_id, [0, 2, 99]),
        Some(2)
    );

    assert_eq!(
        controller.selected_paths(pane_id),
        Some(vec![
            PathBuf::from("/tmp/a/a.txt"),
            PathBuf::from("/tmp/a/c.txt")
        ])
    );
    assert_eq!(
        controller.selection_anchor_path(pane_id),
        Some(PathBuf::from("/tmp/a/a.txt"))
    );
    assert_eq!(
        controller.selection_active_path(pane_id),
        Some(PathBuf::from("/tmp/a/a.txt"))
    );
}

#[path = "tests/view_state.rs"]
mod view_state;

fn test_entry(name: &str) -> Entry {
    test_entry_at("/tmp/a", name)
}

fn test_entry_at(parent: &str, name: &str) -> Entry {
    test_entry_with_path(PathBuf::from(parent).join(name))
}

fn test_entry_with_path(path: PathBuf) -> Entry {
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let name_width_units = name.len() as u16;
    Entry::new(EntryData {
        name: Arc::from(name),
        name_width_units,
        target_path: None,
        size_bytes: 0,
        modified_secs: None,
        metadata_complete: true,
        mime_type: None,
        mime_magic_checked: true,
        trash_original_path: None,
        trash_deletion_time: None,
        is_dir: false,
    })
}

fn listing(entries: Vec<Entry>) -> Arc<Vec<Entry>> {
    Arc::new(entries)
}
