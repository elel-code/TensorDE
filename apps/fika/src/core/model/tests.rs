use super::*;
use crate::core::entries::EntryData;
use crate::core::mime::GENERIC_BINARY_MIME;

fn entry(name: &str, is_dir: bool) -> Entry {
    entry_with_metadata(name, is_dir, 0, None)
}

fn entry_with_metadata(
    name: &str,
    is_dir: bool,
    size_bytes: u64,
    modified_secs: Option<u64>,
) -> Entry {
    Entry::new(EntryData {
        name: Arc::from(name),
        name_width_units: name.len() as u16,
        target_path: None,
        size_bytes,
        modified_secs,
        metadata_complete: true,
        mime_type: None,
        mime_magic_checked: true,
        trash_original_path: None,
        trash_deletion_time: None,
        is_dir,
    })
}

fn entry_with_metadata_state(
    name: &str,
    is_dir: bool,
    size_bytes: u64,
    modified_secs: Option<u64>,
    metadata_complete: bool,
) -> Entry {
    Entry::new(EntryData {
        name: Arc::from(name),
        name_width_units: name.len() as u16,
        target_path: None,
        size_bytes,
        modified_secs,
        metadata_complete,
        mime_type: None,
        mime_magic_checked: true,
        trash_original_path: None,
        trash_deletion_time: None,
        is_dir,
    })
}

fn entry_with_mime_state(
    name: &str,
    size_bytes: u64,
    modified_secs: Option<u64>,
    mime_type: &str,
    mime_magic_checked: bool,
) -> Entry {
    Entry::new(EntryData {
        name: Arc::from(name),
        name_width_units: name.len() as u16,
        target_path: None,
        size_bytes,
        modified_secs,
        metadata_complete: true,
        mime_type: Some(Arc::from(mime_type)),
        mime_magic_checked,
        trash_original_path: None,
        trash_deletion_time: None,
        is_dir: false,
    })
}

fn entry_with_target(name: &str, target_path: &str) -> Entry {
    Entry::new(EntryData {
        name: Arc::from(name),
        name_width_units: name.len() as u16,
        target_path: Some(PathBuf::from(target_path)),
        size_bytes: 0,
        modified_secs: None,
        metadata_complete: true,
        mime_type: Some(Arc::from("inode/directory")),
        mime_magic_checked: true,
        trash_original_path: None,
        trash_deletion_time: None,
        is_dir: true,
    })
}

fn trash_entry(name: &str, original_path: &str, deletion_time: &str) -> Entry {
    Entry::new(EntryData {
        name: Arc::from(name),
        name_width_units: name.len() as u16,
        target_path: None,
        size_bytes: 0,
        modified_secs: None,
        metadata_complete: true,
        mime_type: None,
        mime_magic_checked: true,
        trash_original_path: Some(PathBuf::from(original_path)),
        trash_deletion_time: Some(Arc::from(deletion_time)),
        is_dir: false,
    })
}

fn listing(entries: Vec<Entry>) -> Arc<Vec<Entry>> {
    Arc::new(entries)
}

#[test]
fn listing_reset_rebuilds_path_index() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    let signals = model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry("b.txt", false), entry("a", true)]),
    );

    assert_eq!(signals, vec![DirectoryModelSignal::ModelReset]);
    assert_eq!(model.entries()[0].name.as_ref(), "a");
    assert_eq!(model.path_for_index(1), Some(PathBuf::from("/tmp/b.txt")));
    assert_eq!(model.index_of_path(Path::new("/tmp/b.txt")), Some(1));
}

#[test]
fn listing_uses_entry_target_path_for_activation_and_indexing() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("network:///"));
    model.replace_listing(
        PathBuf::from("network:///"),
        listing(vec![entry_with_target("Team Share", "smb://server/share/")]),
    );

    assert_eq!(
        model.path_for_index(0),
        Some(PathBuf::from("smb://server/share/"))
    );
    assert_eq!(
        model.index_of_path(Path::new("smb://server/share/")),
        Some(0)
    );
}

#[test]
fn listing_snapshot_exports_entry_payload_without_item_identity() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry("b.txt", false), entry("a", true)]),
    );
    let first_id = model.entries()[0].id;

    let snapshot = model.listing_snapshot();

    assert_eq!(snapshot.len(), 2);
    assert_eq!(snapshot[0].name.as_ref(), "a");
    assert_eq!(snapshot[1].name.as_ref(), "b.txt");
    assert!(Entry::ptr_eq(&snapshot[0], &model.entries()[0].entry));
    assert_eq!(model.entries()[0].id, first_id);
}

#[test]
fn path_index_reuses_entry_name_storage() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry("b.txt", false), entry("a", true)]),
    );

    assert_eq!(model.index_of_path(Path::new("/tmp/b.txt")), Some(1));

    let cache = model.path_index.borrow();
    let indexed_name = cache
        .index_by_name
        .keys()
        .find(|name| name.as_ref() == "b.txt")
        .expect("indexed file name missing");
    assert!(Arc::ptr_eq(indexed_name, &model.entries()[1].name));
}

#[test]
fn item_id_index_survives_metadata_role_updates() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![
            entry("a.txt", false),
            entry("b.txt", false),
            entry("c.txt", false),
        ]),
    );
    let item_id = model.entries()[1].id;

    assert_eq!(model.index_of_id(item_id), Some(1));
    let indexed_until = model.id_index.borrow().indexed_until;
    let index_generation = model.index_generation;
    let data_generation = model.data_generation;

    model.set_thumbnail_path(item_id, Some(PathBuf::from("/tmp/thumbs/b.png")));

    assert!(model.data_generation > data_generation);
    assert_eq!(model.index_generation, index_generation);
    assert_eq!(model.id_index.borrow().indexed_until, indexed_until);
    assert_eq!(model.index_of_id(item_id), Some(1));
}

#[test]
fn item_id_index_rebuilds_after_structural_changes() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![
            entry("a.txt", false),
            entry("b.txt", false),
            entry("c.txt", false),
        ]),
    );
    let first_id = model.entries()[0].id;
    let second_id = model.entries()[1].id;

    assert_eq!(model.index_of_id(second_id), Some(1));
    assert!(!model.id_index.borrow().index_by_id.is_empty());

    model.apply_items_deleted(&[PathBuf::from("/tmp/a.txt")]);

    assert!(model.id_index.borrow().index_by_id.is_empty());
    assert_eq!(model.index_of_id(first_id), None);
    assert_eq!(model.index_of_id(second_id), Some(0));
}

#[test]
fn delete_emits_removed_ranges() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![
            entry("a", false),
            entry("b", false),
            entry("c", false),
        ]),
    );

    let signals = model.apply_items_deleted(&[PathBuf::from("/tmp/a"), PathBuf::from("/tmp/b")]);

    assert_eq!(
        signals,
        vec![DirectoryModelSignal::ItemsRemoved(vec![ItemRange {
            start: 0,
            len: 2
        }])]
    );
    assert_eq!(model.entries()[0].name.as_ref(), "c");
}

#[test]
fn added_items_merge_into_sorted_model_with_inserted_ranges() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry("b.txt", false), entry("d.txt", false)]),
    );

    let signals = model.apply_items_added(vec![
        entry("a.txt", false),
        entry("c.txt", false),
        entry("e.txt", false),
    ]);

    assert_eq!(
        signals,
        vec![DirectoryModelSignal::ItemsInserted(vec![
            ItemRange { start: 0, len: 1 },
            ItemRange { start: 2, len: 1 },
            ItemRange { start: 4, len: 1 },
        ])]
    );
    let names = model
        .entries()
        .iter()
        .map(|entry| entry.name.as_ref())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["a.txt", "b.txt", "c.txt", "d.txt", "e.txt"]);
}

#[test]
fn first_added_batch_uses_inserted_range_without_model_reset() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));

    let signals = model.apply_items_added(vec![entry("b.txt", false), entry("a.txt", false)]);

    assert_eq!(
        signals,
        vec![DirectoryModelSignal::ItemsInserted(vec![ItemRange {
            start: 0,
            len: 2,
        }])]
    );
    let names = model
        .entries()
        .iter()
        .map(|entry| entry.name.as_ref())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["a.txt", "b.txt"]);
}

#[test]
fn tail_added_batch_uses_single_final_inserted_range() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(PathBuf::from("/tmp"), listing(vec![entry("a.txt", false)]));

    let signals = model.apply_items_added(vec![entry("c.txt", false), entry("b.txt", false)]);

    assert_eq!(
        signals,
        vec![DirectoryModelSignal::ItemsInserted(vec![ItemRange {
            start: 1,
            len: 2,
        }])]
    );
    let names = model
        .entries()
        .iter()
        .map(|entry| entry.name.as_ref())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["a.txt", "b.txt", "c.txt"]);
}

#[test]
fn full_reload_retains_item_identity_for_same_path() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry("a.txt", false), entry("b.txt", false)]),
    );
    let original_a = model.entries()[0].id;
    let original_b = model.entries()[1].id;

    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry("a.txt", false), entry("b.txt", false)]),
    );

    assert_eq!(model.entries()[0].id, original_a);
    assert_eq!(model.entries()[1].id, original_b);
    assert_eq!(model.index_of_id(original_a), Some(0));
    assert_eq!(model.index_of_id(original_b), Some(1));
}

#[test]
fn same_listing_reload_updates_metadata_without_rebuilding_indexes() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![
            entry_with_metadata("a.txt", false, 1, Some(10)),
            entry_with_metadata("b.txt", false, 2, Some(20)),
        ]),
    );
    let b_id = model.entries()[1].id;
    assert_eq!(model.index_of_path(Path::new("/tmp/b.txt")), Some(1));
    assert_eq!(model.index_of_id(b_id), Some(1));
    let indexed_name = model
        .path_index
        .borrow()
        .index_by_name
        .keys()
        .find(|name| name.as_ref() == "b.txt")
        .cloned()
        .expect("indexed file name missing");
    let path_indexed_until = model.path_index.borrow().indexed_until;
    let id_indexed_until = model.id_index.borrow().indexed_until;
    let index_generation = model.index_generation;

    let signals = model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![
            entry_with_metadata("a.txt", false, 10, Some(100)),
            entry_with_metadata("b.txt", false, 20, Some(200)),
        ]),
    );

    assert_eq!(
        signals,
        vec![DirectoryModelSignal::ItemsChanged(
            vec![ItemRange { start: 0, len: 2 }],
            ChangedRoles::metadata(),
        )]
    );
    assert_eq!(model.index_generation, index_generation);
    assert_eq!(model.path_index.borrow().indexed_until, path_indexed_until);
    assert_eq!(model.id_index.borrow().indexed_until, id_indexed_until);
    assert_eq!(model.index_of_path(Path::new("/tmp/b.txt")), Some(1));
    assert_eq!(model.index_of_id(b_id), Some(1));
    assert_eq!(model.entries()[1].effective_size_bytes(), 20);
    assert_eq!(model.entries()[1].effective_modified_secs(), Some(200));
    assert!(Arc::ptr_eq(&indexed_name, &model.entries()[1].name));
}

#[test]
fn same_listing_lightweight_reload_drops_stale_visible_metadata() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry_with_metadata(
            "notes.txt",
            false,
            512,
            Some(100),
        )]),
    );
    let item_id = model.entries()[0].id;
    let thumbnail_path = PathBuf::from("/tmp/thumbs/notes.png");
    model.set_thumbnail_path(item_id, Some(thumbnail_path.clone()));

    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry_with_metadata_state(
            "notes.txt",
            false,
            0,
            None,
            false,
        )]),
    );

    let entry = &model.entries()[0];
    assert_eq!(entry.id, item_id);
    assert!(!entry.entry.metadata_complete);
    assert!(!entry.effective_metadata_complete());
    assert!(!entry.metadata_refresh_pending);
    assert_eq!(entry.effective_size_bytes(), 0);
    assert_eq!(entry.effective_modified_secs(), None);
    assert!(entry.thumbnail_path.is_none());
}

#[test]
fn same_listing_reload_preserves_failed_thumbnail_role_for_unchanged_file() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry_with_mime_state(
            "image.png",
            128,
            Some(42),
            "image/png",
            true,
        )]),
    );
    let item_id = model.entries()[0].id;
    model.set_thumbnail_failed(item_id, true);

    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry_with_mime_state(
            "image.png",
            128,
            Some(42),
            "image/png",
            true,
        )]),
    );

    assert_eq!(model.entries()[0].id, item_id);
    assert!(model.entries()[0].thumbnail_failed);
}

#[test]
fn same_listing_reload_drops_failed_thumbnail_role_when_file_changes() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry_with_mime_state(
            "image.png",
            128,
            Some(42),
            "image/png",
            true,
        )]),
    );
    let item_id = model.entries()[0].id;
    model.set_thumbnail_failed(item_id, true);

    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry_with_mime_state(
            "image.png",
            128,
            Some(43),
            "image/png",
            true,
        )]),
    );

    assert_eq!(model.entries()[0].id, item_id);
    assert!(!model.entries()[0].thumbnail_failed);
}

#[test]
fn same_listing_directory_reload_does_not_enter_metadata_refresh() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry_with_metadata("Documents", true, 0, Some(100))]),
    );
    let item_id = model.entries()[0].id;

    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry_with_metadata_state(
            "Documents",
            true,
            0,
            None,
            false,
        )]),
    );

    let entry = &model.entries()[0];
    assert_eq!(entry.id, item_id);
    assert!(entry.is_dir);
    assert!(!entry.metadata_refresh_pending);
    assert!(entry.metadata_role.is_none());
    assert_eq!(entry.effective_size_bytes(), 0);
}

#[test]
fn size_sorted_lightweight_reload_drops_stale_visible_metadata() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![
            entry_with_metadata("small.txt", false, 1, Some(10)),
            entry_with_metadata("large.txt", false, 100, Some(20)),
        ]),
    );
    model.set_sort(SortDescriptor {
        role: SortRole::Size,
        order: SortOrder::Descending,
        folders_first: true,
        hidden_last: false,
    });
    let large_id = model.entries()[0].id;
    let thumbnail_path = PathBuf::from("/tmp/thumbs/large.png");
    model.set_thumbnail_path(large_id, Some(thumbnail_path.clone()));
    let index_generation = model.index_generation;

    let signals = model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![
            entry_with_metadata_state("small.txt", false, 0, None, false),
            entry_with_metadata_state("large.txt", false, 0, None, false),
        ]),
    );

    assert_eq!(
        signals,
        vec![DirectoryModelSignal::ItemsChanged(
            vec![ItemRange { start: 0, len: 2 }],
            ChangedRoles::metadata(),
        )]
    );
    assert_eq!(model.index_generation, index_generation);
    let large = model
        .entries()
        .iter()
        .find(|entry| entry.id == large_id)
        .unwrap();
    assert_eq!(large.name.as_ref(), "large.txt");
    assert!(!large.entry.metadata_complete);
    assert!(!large.effective_metadata_complete());
    assert!(!large.metadata_refresh_pending);
    assert_eq!(large.effective_size_bytes(), 0);
    assert_eq!(large.effective_modified_secs(), None);
    assert!(large.thumbnail_path.is_none());
}

#[test]
fn refreshed_item_lightweight_update_drops_stale_visible_metadata() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry_with_metadata(
            "notes.txt",
            false,
            512,
            Some(100),
        )]),
    );
    let item_id = model.entries()[0].id;
    let index_generation = model.index_generation;

    let signals = model.apply_items_refreshed(vec![crate::core::directory::RefreshPair {
        old_path: PathBuf::from("/tmp/notes.txt"),
        entry: Some(entry_with_metadata_state(
            "notes.txt",
            false,
            0,
            None,
            false,
        )),
    }]);

    assert_eq!(
        signals,
        vec![DirectoryModelSignal::ItemsChanged(
            vec![ItemRange { start: 0, len: 1 }],
            ChangedRoles::ALL,
        )]
    );
    assert_eq!(model.index_generation, index_generation);
    assert_eq!(model.entries()[0].id, item_id);
    assert!(!model.entries()[0].entry.metadata_complete);
    assert!(!model.entries()[0].effective_metadata_complete());
    assert!(!model.entries()[0].metadata_refresh_pending);
    assert_eq!(model.entries()[0].effective_size_bytes(), 0);
    assert_eq!(model.entries()[0].effective_modified_secs(), None);
}

#[test]
fn metadata_role_update_is_item_and_path_guarded() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry_with_metadata("payload", false, 0, None)]),
    );
    let item_id = model.entries()[0].id;
    let role = EntryMetadataRole {
        size_bytes: 99,
        modified_secs: Some(42),
        mime_type: Some(Arc::from("text/plain")),
        mime_magic_checked: true,
    };

    assert!(
        model
            .set_metadata_role(item_id, Path::new("/tmp/other"), role.clone())
            .is_empty()
    );
    assert_eq!(model.entries()[0].effective_size_bytes(), 0);

    let signals = model.set_metadata_role(item_id, Path::new("/tmp/payload"), role);

    assert_eq!(
        signals,
        vec![DirectoryModelSignal::ItemsChanged(
            vec![ItemRange { start: 0, len: 1 }],
            ChangedRoles::metadata(),
        )]
    );
    assert!(model.entries()[0].effective_metadata_complete());
    assert_eq!(model.entries()[0].effective_size_bytes(), 99);
    assert_eq!(model.entries()[0].effective_modified_secs(), Some(42));
    assert_eq!(
        model.entries()[0].effective_mime_type().map(Arc::as_ref),
        Some("text/plain")
    );
}

#[test]
fn metadata_role_update_clears_stale_thumbnail_when_identity_changes() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry_with_metadata("image.png", false, 12, Some(10))]),
    );
    let item_id = model.entries()[0].id;
    model.set_thumbnail_path(item_id, Some(PathBuf::from("/tmp/thumbs/image.png")));

    let signals = model.set_metadata_role(
        item_id,
        Path::new("/tmp/image.png"),
        EntryMetadataRole {
            size_bytes: 13,
            modified_secs: Some(11),
            mime_type: Some(Arc::from("image/png")),
            mime_magic_checked: true,
        },
    );

    assert_eq!(
        signals,
        vec![DirectoryModelSignal::ItemsChanged(
            vec![ItemRange { start: 0, len: 1 }],
            ChangedRoles::metadata(),
        )]
    );
    assert!(model.entries()[0].thumbnail_path.is_none());
}

#[test]
fn metadata_role_update_preserves_thumbnail_when_only_mime_is_refined() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry_with_mime_state(
            "image.png",
            12,
            Some(10),
            GENERIC_BINARY_MIME,
            false,
        )]),
    );
    let item_id = model.entries()[0].id;
    let thumbnail_path = PathBuf::from("/tmp/thumbs/image.png");
    model.set_thumbnail_path(item_id, Some(thumbnail_path.clone()));

    model.set_metadata_role(
        item_id,
        Path::new("/tmp/image.png"),
        EntryMetadataRole {
            size_bytes: 12,
            modified_secs: Some(10),
            mime_type: Some(Arc::from("image/png")),
            mime_magic_checked: true,
        },
    );

    assert_eq!(
        model.entries()[0].thumbnail_path.as_deref(),
        Some(thumbnail_path.as_path())
    );
}

#[test]
fn metadata_role_update_drops_failed_thumbnail_when_mime_is_refined() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry_with_mime_state(
            "image.png",
            12,
            Some(10),
            GENERIC_BINARY_MIME,
            false,
        )]),
    );
    let item_id = model.entries()[0].id;
    model.set_thumbnail_failed(item_id, true);

    model.set_metadata_role(
        item_id,
        Path::new("/tmp/image.png"),
        EntryMetadataRole {
            size_bytes: 12,
            modified_secs: Some(10),
            mime_type: Some(Arc::from("image/png")),
            mime_magic_checked: true,
        },
    );

    assert!(!model.entries()[0].thumbnail_failed);
}

#[test]
fn metadata_role_update_resorts_size_sorted_model() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![
            entry_with_metadata("small.txt", false, 1, Some(10)),
            entry_with_metadata("large.txt", false, 10, Some(10)),
        ]),
    );
    model.set_sort(SortDescriptor {
        role: SortRole::Size,
        order: SortOrder::Ascending,
        folders_first: true,
        hidden_last: false,
    });
    let small_id = model
        .entries()
        .iter()
        .find(|entry| entry.name.as_ref() == "small.txt")
        .unwrap()
        .id;

    let signals = model.set_metadata_role(
        small_id,
        Path::new("/tmp/small.txt"),
        EntryMetadataRole {
            size_bytes: 20,
            modified_secs: Some(20),
            mime_type: Some(Arc::from("text/plain")),
            mime_magic_checked: true,
        },
    );

    assert_eq!(signals, vec![DirectoryModelSignal::ModelReset]);
    assert_eq!(model.entries()[1].id, small_id);
    assert_eq!(model.entries()[1].effective_size_bytes(), 20);
}

#[test]
fn thumbnail_role_update_keeps_item_identity_and_emits_metadata_change() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry("image.png", false)]),
    );
    let item_id = model.entries()[0].id;
    let thumbnail_path = PathBuf::from("/tmp/thumbs/image.png");

    let signals = model.set_thumbnail_path(item_id, Some(thumbnail_path.clone()));

    assert_eq!(
        signals,
        vec![DirectoryModelSignal::ItemsChanged(
            vec![ItemRange { start: 0, len: 1 }],
            ChangedRoles::metadata(),
        )]
    );
    assert_eq!(model.entries()[0].id, item_id);
    assert_eq!(
        model.entries()[0].thumbnail_path.as_deref(),
        Some(thumbnail_path.as_path())
    );
    assert!(
        model
            .set_thumbnail_path(item_id, Some(thumbnail_path))
            .is_empty()
    );
    assert!(
        model
            .set_thumbnail_path(ItemId(999), Some(PathBuf::from("/tmp/missing.png")))
            .is_empty()
    );
}

#[test]
fn same_listing_reload_keeps_resolved_mime_as_finished_role_when_metadata_matches() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry_with_mime_state(
            "payload",
            12,
            Some(42),
            "text/plain",
            true,
        )]),
    );
    let item_id = model.entries()[0].id;

    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry_with_mime_state(
            "payload",
            12,
            Some(42),
            GENERIC_BINARY_MIME,
            false,
        )]),
    );

    let entry = &model.entries()[0];
    assert_eq!(entry.id, item_id);
    assert_eq!(entry.entry.mime_type.as_deref(), Some(GENERIC_BINARY_MIME));
    assert!(!entry.entry.mime_magic_checked);
    assert!(!entry.metadata_refresh_pending);
    assert_eq!(
        entry.effective_mime_type().map(Arc::as_ref),
        Some("text/plain")
    );
    assert!(entry.effective_mime_magic_checked());
}

#[test]
fn same_listing_reload_drops_resolved_mime_when_metadata_changes() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry_with_mime_state(
            "payload",
            12,
            Some(42),
            "text/plain",
            true,
        )]),
    );
    let item_id = model.entries()[0].id;

    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry_with_mime_state(
            "payload",
            13,
            Some(43),
            GENERIC_BINARY_MIME,
            false,
        )]),
    );

    let entry = &model.entries()[0];
    assert_eq!(entry.id, item_id);
    assert!(!entry.metadata_refresh_pending);
    assert_eq!(
        entry.effective_mime_type().map(Arc::as_ref),
        Some(GENERIC_BINARY_MIME)
    );
    assert!(!entry.effective_mime_magic_checked());
}

#[test]
fn same_listing_reload_preserves_matching_thumbnail_role() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry_with_metadata("image.png", false, 12, Some(100))]),
    );
    let item_id = model.entries()[0].id;
    let thumbnail_path = PathBuf::from("/tmp/thumbs/image.png");
    model.set_thumbnail_path(item_id, Some(thumbnail_path.clone()));

    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry_with_metadata("image.png", false, 12, Some(100))]),
    );

    assert_eq!(model.entries()[0].id, item_id);
    assert_eq!(
        model.entries()[0].thumbnail_path.as_deref(),
        Some(thumbnail_path.as_path())
    );
}

#[test]
fn metadata_change_clears_thumbnail_role() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry_with_metadata("image.png", false, 12, Some(100))]),
    );
    let item_id = model.entries()[0].id;
    model.set_thumbnail_path(item_id, Some(PathBuf::from("/tmp/thumbs/image.png")));

    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry_with_metadata("image.png", false, 13, Some(101))]),
    );

    assert_eq!(model.entries()[0].id, item_id);
    assert!(model.entries()[0].thumbnail_path.is_none());
}

#[test]
fn incomplete_metadata_reload_keeps_thumbnail_role_until_refresh_finishes() {
    let mut model = DirectoryModel::for_directory(PathBuf::from("/tmp"));
    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry_with_metadata("image.png", false, 12, Some(100))]),
    );
    let item_id = model.entries()[0].id;
    let thumbnail_path = PathBuf::from("/tmp/thumbs/image.png");
    model.set_thumbnail_path(item_id, Some(thumbnail_path.clone()));

    model.replace_listing(
        PathBuf::from("/tmp"),
        listing(vec![entry_with_metadata_state(
            "image.png",
            false,
            12,
            Some(100),
            false,
        )]),
    );

    assert_eq!(model.entries()[0].id, item_id);
    assert_eq!(
        model.entries()[0].thumbnail_path.as_deref(),
        Some(thumbnail_path.as_path())
    );
}

include!("tests/sorting.rs");
