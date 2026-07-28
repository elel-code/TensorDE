use super::*;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn streaming_listing_emits_item_batches_before_completed() {
    let root = std::env::temp_dir().join(format!(
        "fika-streaming-listing-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    struct DirGuard(PathBuf);
    impl Drop for DirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _guard = DirGuard(root.clone());
    for index in 0..513 {
        fs::write(root.join(format!("item-{index:03}.txt")), b"x").unwrap();
    }

    let mut event_groups = Vec::new();
    let result = DirectoryLister::read_listing_events_streaming_cancellable(
        PaneId(1),
        Generation(1),
        RequestSerial(1),
        root.clone(),
        LoadMode::Load,
        || false,
        |events| event_groups.push(events),
    );

    assert_eq!(result, Some(()));
    assert!(matches!(
        event_groups.first().and_then(|events| events.first()),
        Some(DirectoryListerEvent::ItemsAdded { .. })
    ));
    assert!(matches!(
        event_groups.last().and_then(|events| events.first()),
        Some(DirectoryListerEvent::ListingCompleted { .. })
    ));
    let entry_count = event_groups
        .iter()
        .flat_map(|events| events.iter())
        .filter_map(|event| {
            if let DirectoryListerEvent::ItemsAdded { entries, .. } = event {
                Some(entries.len())
            } else {
                None
            }
        })
        .sum::<usize>();
    assert_eq!(entry_count, 513);
    assert_eq!(event_groups.len(), 2);
}

#[test]
fn streaming_reload_first_batch_replaces_current_listing() {
    let root = PathBuf::from("/tmp/fika-streaming-reload-replace");
    let mut lister = DirectoryLister::new(PaneId(1), root.clone(), Generation(1));
    let mut model = DirectoryModel::for_directory(root.clone());
    model.replace_listing(
        root.clone(),
        Arc::new(vec![test_entry("ghost.txt"), test_entry("kept.txt")]),
    );

    let started = lister.load_directory(LoadMode::Reload);
    let request_serial = started.request_serial();
    assert!(lister.apply_event_to_model(started, &mut model).is_empty());

    let signals = lister.apply_event_to_model(
        DirectoryListerEvent::ItemsAdded {
            pane_id: PaneId(1),
            generation: Generation(1),
            request_serial,
            path: root.clone(),
            entries: vec![test_entry("kept.txt")],
        },
        &mut model,
    );

    assert_eq!(signals, vec![DirectoryModelSignal::ModelReset]);
    assert_eq!(model_entry_names(&model), vec!["kept.txt"]);

    lister.apply_event_to_model(
        DirectoryListerEvent::ItemsAdded {
            pane_id: PaneId(1),
            generation: Generation(1),
            request_serial,
            path: root,
            entries: vec![test_entry("new.txt")],
        },
        &mut model,
    );

    assert_eq!(model_entry_names(&model), vec!["kept.txt", "new.txt"]);
}

#[test]
fn streaming_reload_completed_without_items_clears_current_listing() {
    let root = PathBuf::from("/tmp/fika-streaming-reload-empty");
    let mut lister = DirectoryLister::new(PaneId(1), root.clone(), Generation(1));
    let mut model = DirectoryModel::for_directory(root.clone());
    model.replace_listing(root.clone(), Arc::new(vec![test_entry("ghost.txt")]));

    let started = lister.load_directory(LoadMode::Reload);
    let request_serial = started.request_serial();
    assert!(lister.apply_event_to_model(started, &mut model).is_empty());

    let signals = lister.apply_event_to_model(
        DirectoryListerEvent::ListingCompleted {
            pane_id: PaneId(1),
            generation: Generation(1),
            request_serial,
            path: root,
        },
        &mut model,
    );

    assert_eq!(signals, vec![DirectoryModelSignal::ModelReset]);
    assert!(model.entries().is_empty());
}

#[test]
fn network_paths_skip_local_watcher_startup() {
    let mut lister = DirectoryLister::new(
        PaneId(1),
        crate::core::network::network_root_path(),
        Generation(1),
    );

    assert!(lister.start_watcher().is_ok());
}

#[test]
fn network_listing_cancellation_stops_before_scan() {
    let result = DirectoryLister::read_listing_events_cancellable(
        PaneId(1),
        Generation(1),
        RequestSerial(1),
        crate::core::network::network_root_path(),
        LoadMode::Load,
        || true,
    );

    assert_eq!(result, None);
}

#[test]
fn watcher_create_maps_to_items_added() {
    let root = Path::new("/tmp/root");
    let delta = WatcherDelta {
        kind: WatcherDeltaKind::Create,
        paths: vec![root.join("new.txt")],
    };

    assert_eq!(
        classify_watcher_delta(root, delta),
        ClassifiedWatcherDelta::ItemsAdded(vec![root.join("new.txt")])
    );
}

#[test]
fn watcher_root_remove_maps_to_current_directory_removed() {
    let root = Path::new("/tmp/root");
    let delta = WatcherDelta {
        kind: WatcherDeltaKind::Remove,
        paths: vec![root.to_path_buf()],
    };

    assert_eq!(
        classify_watcher_delta(root, delta),
        ClassifiedWatcherDelta::CurrentDirectoryRemoved
    );
}

#[test]
fn watcher_child_remove_maps_to_items_deleted() {
    let root = Path::new("/tmp/root");
    let path = root.join("old.txt");
    let delta = WatcherDelta {
        kind: WatcherDeltaKind::Remove,
        paths: vec![path.clone()],
    };

    assert_eq!(
        classify_watcher_delta(root, delta),
        ClassifiedWatcherDelta::ItemsDeleted(vec![path])
    );
}

#[test]
fn watcher_modify_maps_to_items_refreshed() {
    let root = Path::new("/tmp/root");
    let path = root.join("changed.txt");
    let delta = WatcherDelta {
        kind: WatcherDeltaKind::Modify,
        paths: vec![path.clone()],
    };

    assert_eq!(
        classify_watcher_delta(root, delta),
        ClassifiedWatcherDelta::ItemsRefreshed(vec![path])
    );
}

#[test]
fn watcher_access_notify_events_are_ignored() {
    let root = Path::new("/tmp/root");
    let event = Event {
        kind: EventKind::Access(notify::event::AccessKind::Read),
        paths: vec![root.join("shadow")],
        attrs: Default::default(),
    };

    assert_eq!(WatcherDelta::from_notify_event(root, event), None);
}

#[test]
fn watcher_notify_events_without_relevant_paths_are_ignored() {
    let root = Path::new("/tmp/root");
    let event = Event {
        kind: EventKind::Any,
        paths: vec![PathBuf::from("/tmp/other/changed.txt")],
        attrs: Default::default(),
    };

    assert_eq!(WatcherDelta::from_notify_event(root, event), None);
}

#[test]
fn watcher_root_metadata_notify_events_are_ignored() {
    let root = Path::new("/tmp/root");
    let event = Event {
        kind: EventKind::Modify(ModifyKind::Metadata(notify::event::MetadataKind::Any)),
        paths: vec![root.to_path_buf()],
        attrs: Default::default(),
    };

    assert_eq!(WatcherDelta::from_notify_event(root, event), None);
}

#[test]
fn watcher_two_path_rename_maps_to_refresh_pair() {
    let root = Path::new("/tmp/root");
    let from = root.join("before.txt");
    let to = root.join("after.txt");
    let delta = WatcherDelta {
        kind: WatcherDeltaKind::Rename,
        paths: vec![from.clone(), to.clone()],
    };

    assert_eq!(
        classify_watcher_delta(root, delta),
        ClassifiedWatcherDelta::Renamed { from, to }
    );
}

#[test]
fn watcher_partial_rename_uses_full_reload() {
    let root = Path::new("/tmp/root");
    let delta = WatcherDelta {
        kind: WatcherDeltaKind::Rename,
        paths: vec![root.join("only-one-side.txt")],
    };

    assert_eq!(
        classify_watcher_delta(root, delta),
        ClassifiedWatcherDelta::FullReload
    );
}

#[test]
fn unclassified_watcher_delta_uses_full_reload() {
    let root = Path::new("/tmp/root");
    let delta = WatcherDelta {
        kind: WatcherDeltaKind::Rescan,
        paths: Vec::new(),
    };

    assert_eq!(
        classify_watcher_delta(root, delta),
        ClassifiedWatcherDelta::FullReload
    );
}

#[test]
fn watcher_coalesce_merges_adjacent_same_kind_paths() {
    let root = Path::new("/tmp/root");
    let alpha = root.join("alpha.txt");
    let beta = root.join("beta.txt");

    assert_eq!(
        coalesce_watcher_deltas(
            root,
            [
                WatcherDelta {
                    kind: WatcherDeltaKind::Modify,
                    paths: vec![alpha.clone()],
                },
                WatcherDelta {
                    kind: WatcherDeltaKind::Modify,
                    paths: vec![alpha.clone(), beta.clone()],
                },
            ],
        ),
        vec![ClassifiedWatcherDelta::ItemsRefreshed(vec![alpha, beta])]
    );
}

#[test]
fn watcher_coalesce_keeps_order_across_different_delta_kinds() {
    let root = Path::new("/tmp/root");
    let created = root.join("created.txt");
    let modified = root.join("modified.txt");

    assert_eq!(
        coalesce_watcher_deltas(
            root,
            [
                WatcherDelta {
                    kind: WatcherDeltaKind::Create,
                    paths: vec![created.clone()],
                },
                WatcherDelta {
                    kind: WatcherDeltaKind::Modify,
                    paths: vec![modified.clone()],
                },
            ],
        ),
        vec![
            ClassifiedWatcherDelta::ItemsAdded(vec![created]),
            ClassifiedWatcherDelta::ItemsRefreshed(vec![modified])
        ]
    );
}

#[test]
fn watcher_coalesce_full_reload_supersedes_incremental_deltas() {
    let root = Path::new("/tmp/root");

    assert_eq!(
        coalesce_watcher_deltas(
            root,
            [
                WatcherDelta {
                    kind: WatcherDeltaKind::Modify,
                    paths: vec![root.join("changed.txt")],
                },
                WatcherDelta {
                    kind: WatcherDeltaKind::Rescan,
                    paths: Vec::new(),
                },
                WatcherDelta {
                    kind: WatcherDeltaKind::Create,
                    paths: vec![root.join("ignored.txt")],
                },
            ],
        ),
        vec![ClassifiedWatcherDelta::FullReload]
    );
}

#[test]
fn watcher_coalesce_current_directory_removed_supersedes_everything() {
    let root = Path::new("/tmp/root");

    assert_eq!(
        coalesce_watcher_deltas(
            root,
            [
                WatcherDelta {
                    kind: WatcherDeltaKind::Create,
                    paths: vec![root.join("created.txt")],
                },
                WatcherDelta {
                    kind: WatcherDeltaKind::Remove,
                    paths: vec![root.to_path_buf()],
                },
                WatcherDelta {
                    kind: WatcherDeltaKind::Modify,
                    paths: vec![root.join("ignored.txt")],
                },
            ],
        ),
        vec![ClassifiedWatcherDelta::CurrentDirectoryRemoved]
    );
}

fn test_entry(name: &str) -> Entry {
    Entry::new(super::super::entries::EntryData {
        name: Arc::from(name),
        name_width_units: name.len() as u16,
        target_path: None,
        size_bytes: 0,
        modified_secs: None,
        metadata_complete: true,
        trash_original_path: None,
        trash_deletion_time: None,
        mime_type: None,
        mime_magic_checked: true,
        is_dir: false,
    })
}

fn model_entry_names(model: &DirectoryModel) -> Vec<&str> {
    model
        .entries()
        .iter()
        .map(|entry| entry.name.as_ref())
        .collect()
}
