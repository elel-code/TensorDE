use super::*;

#[test]
fn thumbnail_request_queue_schedules_visible_before_deferred() {
    let root = temp_root("queue-visible");
    fs::create_dir_all(&root).unwrap();
    let mut queue = ThumbnailRequestQueue::new();

    assert!(queue.enqueue(test_request(
        &root,
        "deferred.png",
        ItemId(1),
        Generation(1),
        ThumbnailRequestPriority::Deferred,
    )));
    assert!(queue.enqueue(test_request(
        &root,
        "visible.png",
        ItemId(2),
        Generation(1),
        ThumbnailRequestPriority::Visible,
    )));

    let first = queue.pop_next().unwrap();
    assert_eq!(first.item_id(), ItemId(2));
    assert_eq!(first.priority(), ThumbnailRequestPriority::Visible);
    assert_eq!(queue.pop_next().unwrap().item_id(), ItemId(1));
    assert!(queue.is_empty());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn thumbnail_request_uses_entry_metadata_without_restat() {
    let path = PathBuf::from("/tmp/fika-thumbnail-metadata-missing.png");

    let request = ThumbnailRequest::from_entry_metadata(
        PaneId(1),
        Generation(2),
        ItemId(3),
        path.clone(),
        42,
        ThumbnailRequestPriority::Visible,
    )
    .unwrap();
    assert_eq!(request.modified_secs(), 42);
    assert_eq!(
        request.uri(),
        "file:///tmp/fika-thumbnail-metadata-missing.png"
    );

    assert!(
        ThumbnailRequest::new(
            PaneId(1),
            Generation(2),
            ItemId(3),
            path,
            ThumbnailRequestPriority::Visible,
        )
        .is_none()
    );
}

#[test]
fn thumbnail_request_queue_deduplicates_and_promotes_visible_requests() {
    let root = temp_root("queue-dedup");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("same.png");
    fs::write(&path, b"source").unwrap();
    let mut queue = ThumbnailRequestQueue::new();

    assert!(queue.enqueue_path(
        PaneId(1),
        Generation(1),
        ItemId(1),
        path.clone(),
        ThumbnailRequestPriority::Deferred,
    ));
    assert!(!queue.enqueue_path(
        PaneId(1),
        Generation(1),
        ItemId(1),
        path.clone(),
        ThumbnailRequestPriority::Deferred,
    ));
    assert!(queue.enqueue_path(
        PaneId(1),
        Generation(1),
        ItemId(1),
        path,
        ThumbnailRequestPriority::Visible,
    ));
    assert_eq!(queue.len(), 1);

    let request = queue.pop_next().unwrap();
    assert_eq!(request.item_id(), ItemId(1));
    assert_eq!(request.priority(), ThumbnailRequestPriority::Visible);
    assert!(queue.is_empty());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn thumbnail_request_queue_cancels_stale_generations_for_navigation() {
    let root = temp_root("queue-cancel");
    fs::create_dir_all(&root).unwrap();
    let mut queue = ThumbnailRequestQueue::new();

    assert!(queue.enqueue(test_request(
        &root,
        "old.png",
        ItemId(1),
        Generation(1),
        ThumbnailRequestPriority::Visible,
    )));
    assert!(queue.enqueue(test_request(
        &root,
        "current.png",
        ItemId(2),
        Generation(2),
        ThumbnailRequestPriority::Deferred,
    )));
    assert!(
        queue.enqueue(
            ThumbnailRequest::new(
                PaneId(2),
                Generation(1),
                ItemId(3),
                write_source(&root, "other-pane.png"),
                ThumbnailRequestPriority::Visible,
            )
            .unwrap(),
        )
    );

    assert_eq!(queue.cancel_stale_generations(PaneId(1), Generation(2)), 1);
    assert_eq!(queue.len(), 2);
    assert_eq!(queue.pop_next().unwrap().pane_id(), PaneId(2));
    let current = queue.pop_next().unwrap();
    assert_eq!(current.pane_id(), PaneId(1));
    assert_eq!(current.generation(), Generation(2));
    assert!(queue.is_empty());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn thumbnail_request_queue_contains_tracks_pending_requests() {
    let root = temp_root("queue-contains");
    fs::create_dir_all(&root).unwrap();
    let mut queue = ThumbnailRequestQueue::new();
    let request = test_request(
        &root,
        "visible.png",
        ItemId(1),
        Generation(1),
        ThumbnailRequestPriority::Visible,
    );

    assert!(!queue.contains(&request));
    assert!(queue.enqueue(request.clone()));
    assert!(queue.contains(&request));

    let popped = queue.pop_next().unwrap();
    assert_eq!(popped.item_id(), request.item_id());
    assert!(!queue.contains(&request));
    assert!(queue.is_empty());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn thumbnail_request_queue_cancels_only_matching_deferred_requests() {
    let root = temp_root("queue-cancel-deferred");
    fs::create_dir_all(&root).unwrap();
    let mut queue = ThumbnailRequestQueue::new();
    let visible = test_request(
        &root,
        "visible.png",
        ItemId(1),
        Generation(1),
        ThumbnailRequestPriority::Visible,
    );
    let keep_deferred = test_request(
        &root,
        "keep-deferred.png",
        ItemId(2),
        Generation(1),
        ThumbnailRequestPriority::Deferred,
    );
    let remove_deferred = test_request(
        &root,
        "remove-deferred.png",
        ItemId(3),
        Generation(1),
        ThumbnailRequestPriority::Deferred,
    );

    assert!(queue.enqueue(visible.clone()));
    assert!(queue.enqueue(keep_deferred.clone()));
    assert!(queue.enqueue(remove_deferred.clone()));

    let removed = queue.cancel_deferred_matching(|request| request.item_id() == ItemId(3));

    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].item_id(), ItemId(3));
    assert!(queue.contains(&visible));
    assert!(queue.contains(&keep_deferred));
    assert!(!queue.contains(&remove_deferred));
    assert_eq!(queue.len(), 2);
    assert_eq!(queue.pop_next().unwrap().item_id(), ItemId(1));
    assert_eq!(queue.pop_next().unwrap().item_id(), ItemId(2));
    assert!(queue.is_empty());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn thumbnail_failure_marker_uses_freedesktop_fail_path() {
    let root = temp_root("failure");
    let uri = "file:///tmp/broken.png";
    let path = record_thumbnail_failure(&root, uri, 123).unwrap();

    assert_eq!(path, thumbnail_failure_path(&root, uri));
    assert!(thumbnail_failure_is_cached(&root, uri, 123));
    assert!(!thumbnail_failure_is_cached(&root, uri, 124));
    let metadata = thumbnail_metadata(&path).unwrap();
    assert_eq!(metadata.uri.as_deref(), Some(uri));
    assert_eq!(metadata.mtime, Some(123));
    let bytes = fs::read(path).unwrap();
    assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn thumbnail_failure_marker_overwrites_stale_metadata() {
    let root = temp_root("failure-stale");
    let uri = "file:///tmp/changed.png";
    let path = record_thumbnail_failure(&root, uri, 123).unwrap();
    assert!(thumbnail_failure_is_cached(&root, uri, 123));

    assert_eq!(record_thumbnail_failure(&root, uri, 456).unwrap(), path);

    assert!(!thumbnail_failure_is_cached(&root, uri, 123));
    assert!(thumbnail_failure_is_cached(&root, uri, 456));
    let metadata = thumbnail_metadata(&path).unwrap();
    assert_eq!(metadata.uri.as_deref(), Some(uri));
    assert_eq!(metadata.mtime, Some(456));

    let _ = fs::remove_dir_all(root);
}
