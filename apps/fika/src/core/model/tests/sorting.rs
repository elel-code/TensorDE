#[test]
fn split_models_share_listing_payload_without_entry_level_identity() {
    let listing = listing(vec![entry("shared.txt", false)]);
    let mut first = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    let mut second = DirectoryModel::for_directory(PathBuf::from("/tmp"));

    first.replace_listing(PathBuf::from("/tmp"), Arc::clone(&listing));
    second.replace_listing(PathBuf::from("/tmp"), Arc::clone(&listing));

    assert!(first.entries()[0].id.is_assigned());
    assert!(second.entries()[0].id.is_assigned());
    assert!(Entry::ptr_eq(
        &first.entries()[0].entry,
        &second.entries()[0].entry
    ));
}

#[test]
fn fork_for_pane_shares_payload_but_not_model_entries() {
    let mut source = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    source.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry("a.txt", false), entry("b.txt", false)]),
    );

    let mut fork = source.fork_for_pane();
    assert_eq!(fork.len(), source.len());
    assert!(Entry::ptr_eq(
        &source.entries()[0].entry,
        &fork.entries()[0].entry
    ));
    assert!(Entry::ptr_eq(
        &source.entries()[1].entry,
        &fork.entries()[1].entry
    ));

    fork.apply_items_deleted(&[PathBuf::from("/tmp/a.txt")]);

    assert_eq!(source.len(), 2);
    assert_eq!(fork.len(), 1);
    assert_eq!(source.index_of_path(Path::new("/tmp/a.txt")), Some(0));
    assert_eq!(fork.index_of_path(Path::new("/tmp/a.txt")), None);
    assert!(Entry::ptr_eq(
        &source.entries()[1].entry,
        &fork.entries()[0].entry
    ));
}

#[test]
fn refresh_rename_retains_item_identity_from_old_path() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry("old.txt", false)]),
    );
    let original = model.entries()[0].id;

    model.apply_items_refreshed(vec![crate::core::directory::RefreshPair {
        old_path: PathBuf::from("/tmp/old.txt"),
        entry: Some(entry("new.txt", false)),
    }]);

    assert_eq!(model.entries()[0].id, original);
    assert_eq!(model.path_for_index(0), Some(PathBuf::from("/tmp/new.txt")));
    assert_eq!(model.index_of_path(Path::new("/tmp/old.txt")), None);
    assert_eq!(model.index_of_id(original), Some(0));
}

#[test]
fn set_sort_by_size_keeps_directories_first_and_preserves_item_identity() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![
            entry_with_metadata("small.txt", false, 1, None),
            entry_with_metadata("folder-b", true, 0, None),
            entry_with_metadata("big.txt", false, 100, None),
            entry_with_metadata("folder-a", true, 0, None),
        ]),
    );
    let big_id = model
        .entries()
        .iter()
        .find(|entry| entry.name.as_ref() == "big.txt")
        .unwrap()
        .id;

    let signals = model.set_sort(SortDescriptor {
        role: SortRole::Size,
        order: SortOrder::Descending,
        ..SortDescriptor::default()
    });

    assert_eq!(signals, vec![DirectoryModelSignal::SortChanged]);
    assert_eq!(
        model
            .entries()
            .iter()
            .map(|entry| entry.name.as_ref())
            .collect::<Vec<_>>(),
        vec!["folder-a", "folder-b", "big.txt", "small.txt"]
    );
    assert_eq!(model.index_of_id(big_id), Some(2));
}

#[test]
fn set_sort_by_modified_uses_model_role_order() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![
            entry_with_metadata("new.txt", false, 0, Some(30)),
            entry_with_metadata("unknown.txt", false, 0, None),
            entry_with_metadata("old.txt", false, 0, Some(10)),
        ]),
    );

    model.set_sort(SortDescriptor {
        role: SortRole::Modified,
        order: SortOrder::Ascending,
        ..SortDescriptor::default()
    });

    assert_eq!(
        model
            .entries()
            .iter()
            .map(|entry| entry.name.as_ref())
            .collect::<Vec<_>>(),
        vec!["unknown.txt", "old.txt", "new.txt"]
    );
}

#[test]
fn folders_first_can_be_disabled_for_name_sorting() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![
            entry("z-dir", true),
            entry("b-file.txt", false),
            entry("a-dir", true),
            entry("a-file.txt", false),
        ]),
    );

    model.set_sort(SortDescriptor {
        role: SortRole::Name,
        order: SortOrder::Ascending,
        folders_first: false,
        hidden_last: false,
    });

    assert_eq!(
        model
            .entries()
            .iter()
            .map(|entry| entry.name.as_ref())
            .collect::<Vec<_>>(),
        vec!["a-dir", "a-file.txt", "b-file.txt", "z-dir"]
    );
}

#[test]
fn size_sort_keeps_directories_first_even_when_folders_first_is_disabled() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![
            entry_with_metadata("large-file.txt", false, 100, None),
            entry_with_metadata("folder-b", true, 0, None),
            entry_with_metadata("small-file.txt", false, 1, None),
            entry_with_metadata("folder-a", true, 0, None),
        ]),
    );

    model.set_sort(SortDescriptor {
        role: SortRole::Size,
        order: SortOrder::Descending,
        folders_first: false,
        hidden_last: false,
    });

    assert_eq!(
        model
            .entries()
            .iter()
            .map(|entry| entry.name.as_ref())
            .collect::<Vec<_>>(),
        vec!["folder-a", "folder-b", "large-file.txt", "small-file.txt"]
    );
}

#[test]
fn hidden_last_sorts_hidden_entries_after_visible_entries() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![
            entry(".hidden-file.txt", false),
            entry("visible-file.txt", false),
            entry(".hidden-folder", true),
            entry("visible-folder", true),
        ]),
    );

    model.set_sort(SortDescriptor {
        role: SortRole::Name,
        order: SortOrder::Ascending,
        folders_first: true,
        hidden_last: true,
    });

    assert_eq!(
        model
            .entries()
            .iter()
            .map(|entry| entry.name.as_ref())
            .collect::<Vec<_>>(),
        vec![
            "visible-folder",
            "visible-file.txt",
            ".hidden-folder",
            ".hidden-file.txt"
        ]
    );
}

#[test]
fn trash_listing_sorts_by_deletion_time_and_retains_identity_after_reload() {
    let trash_dir = file_ops::trash_files_dir();
    let mut model = DirectoryModel::for_directory(trash_dir.clone());
    model.replace_listing(
        trash_dir.clone(),
        listing(vec![
            trash_entry("old.txt", "/tmp/old.txt", "2026-06-01T10:00:00"),
            trash_entry("new.txt", "/tmp/new.txt", "2026-06-03T10:00:00"),
        ]),
    );
    let new_id = model.entries()[0].id;
    let old_id = model.entries()[1].id;

    assert_eq!(model.entries()[0].name.as_ref(), "new.txt");
    assert_eq!(model.entries()[1].name.as_ref(), "old.txt");

    let signals = model.replace_listing(
        trash_dir.clone(),
        listing(vec![
            trash_entry("old.txt", "/tmp/old.txt", "2026-06-05T10:00:00"),
            trash_entry("new.txt", "/tmp/new.txt", "2026-06-03T10:00:00"),
        ]),
    );

    assert_eq!(signals, vec![DirectoryModelSignal::ModelReset]);
    assert_eq!(model.entries()[0].name.as_ref(), "old.txt");
    assert_eq!(model.entries()[0].id, old_id);
    assert_eq!(model.entries()[1].name.as_ref(), "new.txt");
    assert_eq!(model.entries()[1].id, new_id);
}

#[test]
fn trash_listing_can_sort_by_original_path_role() {
    let trash_dir = file_ops::trash_files_dir();
    let mut model = DirectoryModel::for_directory(trash_dir.clone());
    model.replace_listing(
        trash_dir,
        listing(vec![
            trash_entry("beta.txt", "/tmp/beta/beta.txt", "2026-06-03T10:00:00"),
            trash_entry("alpha.txt", "/tmp/alpha/alpha.txt", "2026-06-01T10:00:00"),
            trash_entry("gamma.txt", "/tmp/gamma/gamma.txt", "2026-06-02T10:00:00"),
        ]),
    );

    let signals = model.set_sort(SortDescriptor {
        role: SortRole::TrashOriginalPath,
        order: SortOrder::Ascending,
        folders_first: true,
        hidden_last: false,
    });

    assert_eq!(signals, vec![DirectoryModelSignal::SortChanged]);
    assert_eq!(
        model
            .entries()
            .iter()
            .map(|entry| entry.name.as_ref())
            .collect::<Vec<_>>(),
        vec!["alpha.txt", "beta.txt", "gamma.txt"]
    );
}

#[test]
fn trash_metadata_refresh_resorts_and_keeps_item_identity() {
    let trash_dir = file_ops::trash_files_dir();
    let mut model = DirectoryModel::for_directory(trash_dir.clone());
    model.replace_listing(
        trash_dir.clone(),
        listing(vec![
            trash_entry("old.txt", "/tmp/old.txt", "2026-06-01T10:00:00"),
            trash_entry("new.txt", "/tmp/new.txt", "2026-06-03T10:00:00"),
        ]),
    );
    let old_id = model.entries()[1].id;

    let signals = model.apply_items_refreshed(vec![crate::core::directory::RefreshPair {
        old_path: trash_dir.join("old.txt"),
        entry: Some(trash_entry(
            "old.txt",
            "/tmp/old.txt",
            "2026-06-05T10:00:00",
        )),
    }]);

    assert_eq!(signals, vec![DirectoryModelSignal::ModelReset]);
    assert_eq!(model.entries()[0].name.as_ref(), "old.txt");
    assert_eq!(model.entries()[0].id, old_id);
    assert_eq!(
        model.entries()[0].trash_deletion_time.as_deref(),
        Some("2026-06-05T10:00:00")
    );
}
