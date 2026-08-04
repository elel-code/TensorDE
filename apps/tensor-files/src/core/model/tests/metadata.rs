use super::*;

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
