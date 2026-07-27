use super::*;
use std::process;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[test]
fn thumbnail_uri_percent_encodes_file_path() {
    let uri = thumbnail_uri_for_path(Path::new("/tmp/Fika Test/value#1.txt")).unwrap();
    assert_eq!(uri, "file:///tmp/Fika%20Test/value%231.txt");
    assert!(thumbnail_uri_for_path(Path::new("relative.txt")).is_none());
}

#[test]
fn freedesktop_thumbnail_hash_uses_md5_uri() {
    assert_eq!(thumbnail_cache_key(""), "d41d8cd98f00b204e9800998ecf8427e");
    assert_eq!(
        thumbnail_cache_key("abc"),
        "900150983cd24fb0d6963f7d28e17f72"
    );
    assert_eq!(
        thumbnail_cache_key("file:///tmp/Fika%20Test/value%231.txt"),
        "4869a68b8abd00bef4bb8d34392b25c7"
    );
}

#[test]
fn thumbnail_cache_lookup_prefers_normal_before_large() {
    let root = temp_root("lookup");
    let uri = "file:///tmp/image.png";
    let large = thumbnail_cache_path(&root, ThumbnailSize::Large, uri);
    let normal = thumbnail_cache_path(&root, ThumbnailSize::Normal, uri);
    fs::create_dir_all(large.parent().unwrap()).unwrap();
    fs::write(&large, test_thumbnail_png(uri, 123)).unwrap();

    let large_hit = cached_thumbnail_for_uri(&root, uri).unwrap();
    assert_eq!(large_hit.size(), ThumbnailSize::Large);
    assert_eq!(large_hit.path(), large.as_path());

    fs::create_dir_all(normal.parent().unwrap()).unwrap();
    fs::write(&normal, test_thumbnail_png(uri, 123)).unwrap();
    let normal_hit = cached_thumbnail_for_uri(&root, uri).unwrap();
    assert_eq!(normal_hit.size(), ThumbnailSize::Normal);
    assert_eq!(normal_hit.path(), normal.as_path());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn thumbnail_size_for_display_px_prefers_sharper_buckets() {
    assert_eq!(ThumbnailSize::for_display_px(16), ThumbnailSize::Normal);
    assert_eq!(ThumbnailSize::for_display_px(32), ThumbnailSize::Normal);
    assert_eq!(ThumbnailSize::for_display_px(48), ThumbnailSize::Large);
    assert_eq!(ThumbnailSize::for_display_px(96), ThumbnailSize::Large);
    assert_eq!(ThumbnailSize::for_display_px(128), ThumbnailSize::XLarge);
    assert_eq!(ThumbnailSize::for_display_px(256), ThumbnailSize::XXLarge);
}

#[test]
fn thumbnail_size_for_prepared_raster_does_not_apply_display_bias_twice() {
    assert_eq!(ThumbnailSize::for_raster_px(128), ThumbnailSize::Normal);
    assert_eq!(ThumbnailSize::for_raster_px(256), ThumbnailSize::Large);
    assert_eq!(ThumbnailSize::for_raster_px(512), ThumbnailSize::XLarge);
    assert_eq!(ThumbnailSize::for_raster_px(1024), ThumbnailSize::XXLarge);
}

#[test]
fn thumbnail_cache_lookup_prefers_requested_then_sharper() {
    let root = temp_root("lookup-prefer");
    let uri = "file:///tmp/photo.jpg";
    let normal = thumbnail_cache_path(&root, ThumbnailSize::Normal, uri);
    let large = thumbnail_cache_path(&root, ThumbnailSize::Large, uri);
    let x_large = thumbnail_cache_path(&root, ThumbnailSize::XLarge, uri);
    for path in [&normal, &large, &x_large] {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
    }
    fs::write(&normal, test_thumbnail_png(uri, 1)).unwrap();
    fs::write(&large, test_thumbnail_png(uri, 1)).unwrap();
    fs::write(&x_large, test_thumbnail_png(uri, 1)).unwrap();

    // Preferred Large hits first even when sharper x-large also exists.
    let hit = cached_thumbnail_preferring(&root, uri, Some(1), ThumbnailSize::Large).unwrap();
    assert_eq!(hit.size(), ThumbnailSize::Large);
    assert_eq!(hit.path(), large.as_path());

    // Preferred XLarge hits its own bucket.
    let hit = cached_thumbnail_preferring(&root, uri, Some(1), ThumbnailSize::XLarge).unwrap();
    assert_eq!(hit.size(), ThumbnailSize::XLarge);

    let _ = fs::remove_dir_all(root);
}

#[test]
fn thumbnail_cache_lookup_rejects_mismatched_metadata() {
    let root = temp_root("mismatch");
    let file = root.join("image.png");
    fs::create_dir_all(&root).unwrap();
    fs::write(&file, b"source").unwrap();
    let uri = thumbnail_uri_for_path(&file).unwrap();
    let mtime = fs::metadata(&file)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let thumbnail = thumbnail_cache_path(&root, ThumbnailSize::Normal, &uri);
    fs::create_dir_all(thumbnail.parent().unwrap()).unwrap();
    fs::write(
        &thumbnail,
        test_thumbnail_png("file:///tmp/other.png", mtime),
    )
    .unwrap();
    assert!(cached_thumbnail_for_path(&root, &file).is_none());

    fs::write(&thumbnail, test_thumbnail_png(&uri, mtime + 1)).unwrap();
    assert!(cached_thumbnail_for_path(&root, &file).is_none());

    fs::write(&thumbnail, test_thumbnail_png(&uri, mtime)).unwrap();
    assert_eq!(
        cached_thumbnail_for_path(&root, &file).unwrap().path(),
        thumbnail.as_path()
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn thumbnail_metadata_reads_freedesktop_text_chunks() {
    let uri = "file:///tmp/image.png";
    let metadata = thumbnail_metadata_from_bytes(&test_thumbnail_png(uri, 42)).unwrap();

    assert_eq!(metadata.uri.as_deref(), Some(uri));
    assert_eq!(metadata.mtime, Some(42));
}

#[test]
fn cached_thumbnail_for_request_uses_request_mtime_without_restat() {
    let root = temp_root("request-cache");
    let path = PathBuf::from("/tmp/fika-thumbnail-request-missing.png");
    let request = ThumbnailRequest::from_entry_metadata(
        PaneId(1),
        Generation(1),
        ItemId(1),
        path,
        42,
        ThumbnailRequestPriority::Visible,
    )
    .unwrap();
    let thumbnail = thumbnail_cache_path(&root, ThumbnailSize::Normal, request.uri());
    fs::create_dir_all(thumbnail.parent().unwrap()).unwrap();
    fs::write(&thumbnail, test_thumbnail_png(request.uri(), 42)).unwrap();

    assert_eq!(
        cached_thumbnail_for_request(&root, &request)
            .unwrap()
            .path(),
        thumbnail.as_path()
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn external_thumbnailer_commands_match_file_kind() {
    let image = external_thumbnailer_commands_for_path(
        Path::new("/tmp/photo.JPG"),
        Path::new("/tmp/out.png"),
        ThumbnailSize::Normal,
    );
    assert_eq!(image.len(), 1);
    assert_eq!(image[0].program(), "gdk-pixbuf-thumbnailer");
    assert_eq!(image[0].args()[0], OsString::from("-s"));
    assert_eq!(image[0].args()[1], OsString::from("128"));

    let video = external_thumbnailer_commands_for_path(
        Path::new("/tmp/movie.webm"),
        Path::new("/tmp/out.png"),
        ThumbnailSize::Large,
    );
    assert_eq!(video.len(), 2);
    assert_eq!(video[0].program(), "ffmpegthumbnailer");
    assert_eq!(video[1].program(), "totem-video-thumbnailer");
    assert!(video[0].args().contains(&OsString::from("256")));

    let document = external_thumbnailer_commands_for_path(
        Path::new("/tmp/document.pdf"),
        Path::new("/tmp/out.png"),
        ThumbnailSize::Normal,
    );
    assert_eq!(document.len(), 1);
    assert_eq!(document[0].program(), "evince-thumbnailer");

    assert!(
        external_thumbnailer_commands_for_path(
            Path::new("/tmp/archive.zip"),
            Path::new("/tmp/out.png"),
            ThumbnailSize::Normal,
        )
        .is_empty()
    );
}

#[test]
fn thumbnail_preview_filter_matches_dolphin_preview_candidates() {
    assert!(!thumbnail_request_may_have_preview(
        Path::new("/tmp/notes.txt"),
        Some("text/plain")
    ));
    assert!(thumbnail_request_may_have_preview(
        Path::new("/tmp/photo"),
        Some("image/png")
    ));
    assert!(thumbnail_request_may_have_preview(
        Path::new("/tmp/photo.png"),
        Some("application/octet-stream")
    ));
    assert!(thumbnail_request_may_have_preview(
        Path::new("/tmp/clip.avifs"),
        Some("application/octet-stream")
    ));
    assert!(thumbnail_request_may_have_preview(
        Path::new("/tmp/setup.exe"),
        Some("application/vnd.microsoft.portable-executable")
    ));
    assert!(thumbnail_request_may_have_preview(
        Path::new("/tmp/setup.exe"),
        Some("application/octet-stream")
    ));
    assert!(!thumbnail_request_may_have_preview(
        Path::new("/tmp/clip.mp4"),
        Some("video/mp4")
    ));
    assert!(!thumbnail_request_may_have_preview(
        Path::new("/tmp/clip.mp4"),
        Some("application/octet-stream")
    ));
    assert!(!thumbnail_request_may_have_preview(
        Path::new("/tmp/archive.zip"),
        Some("application/zip")
    ));
}

#[test]
fn thumbnailer_registry_parses_and_expands_desktop_exec() {
    let root = temp_root("thumbnailer-registry");
    let dir = root.join("thumbnailers");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("custom.thumbnailer"),
        "[Thumbnailer Entry]\n\
         Exec=custom-thumbnailer --size %s --input %i --uri %u --output %o\n\
         MimeType=image/png;image/jpeg;\n",
    )
    .unwrap();
    let registry = ThumbnailerRegistry::load_from_dirs([dir]);
    let request = ThumbnailRequest::from_entry_metadata_with_mime(
        PaneId(1),
        Generation(1),
        ItemId(1),
        PathBuf::from("/tmp/photo.png"),
        42,
        Some("image/png".to_string()),
        ThumbnailRequestPriority::Visible,
    )
    .unwrap();

    let commands =
        registry.commands_for_request(&request, Path::new("/tmp/out.png"), ThumbnailSize::Large);

    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].program(), "custom-thumbnailer");
    assert_eq!(
        commands[0].args(),
        &[
            OsString::from("--size"),
            OsString::from("256"),
            OsString::from("--input"),
            OsString::from("/tmp/photo.png"),
            OsString::from("--uri"),
            OsString::from("file:///tmp/photo.png"),
            OsString::from("--output"),
            OsString::from("/tmp/out.png"),
        ]
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn thumbnailer_exec_expands_common_freedesktop_file_field_codes() {
    let root = temp_root("thumbnailer-field-codes");
    let dir = root.join("thumbnailers");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("field-codes.thumbnailer"),
        "[Thumbnailer Entry]\n\
         Exec=field-thumb --file %f --files %F --url %U --dir %d --dirs %D --name %n --names %N --literal %% \"--quoted=%f\"\n\
         MimeType=image/png;\n",
    )
    .unwrap();
    let registry = ThumbnailerRegistry::load_from_dirs([dir]);
    let request = ThumbnailRequest::from_entry_metadata_with_mime(
        PaneId(1),
        Generation(1),
        ItemId(1),
        PathBuf::from("/tmp/Fika Test/photo one.png"),
        42,
        Some("image/png".to_string()),
        ThumbnailRequestPriority::Visible,
    )
    .unwrap();

    let commands = registry.commands_for_request(
        &request,
        Path::new("/tmp/out image.png"),
        ThumbnailSize::Normal,
    );

    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].program(), "field-thumb");
    assert_eq!(
        commands[0].args(),
        &[
            OsString::from("--file"),
            OsString::from("/tmp/Fika Test/photo one.png"),
            OsString::from("--files"),
            OsString::from("/tmp/Fika Test/photo one.png"),
            OsString::from("--url"),
            OsString::from("file:///tmp/Fika%20Test/photo%20one.png"),
            OsString::from("--dir"),
            OsString::from("/tmp/Fika Test"),
            OsString::from("--dirs"),
            OsString::from("/tmp/Fika Test"),
            OsString::from("--name"),
            OsString::from("photo one.png"),
            OsString::from("--names"),
            OsString::from("photo one.png"),
            OsString::from("--literal"),
            OsString::from("%"),
            OsString::from("--quoted=/tmp/Fika Test/photo one.png"),
        ]
    );

    let _ = fs::remove_dir_all(root);
}

#[test]
fn thumbnailer_registry_matches_wildcard_mime_before_extension_fallback() {
    let root = temp_root("thumbnailer-wildcard");
    let dir = root.join("thumbnailers");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("wildcard.thumbnailer"),
        "[Thumbnailer Entry]\nExec=wild-thumb %i %o\nMimeType=image/*;\n",
    )
    .unwrap();
    let registry = ThumbnailerRegistry::load_from_dirs([dir]);
    let request = ThumbnailRequest::from_entry_metadata_with_mime(
        PaneId(1),
        Generation(1),
        ItemId(1),
        PathBuf::from("/tmp/photo.png"),
        42,
        Some("image/png".to_string()),
        ThumbnailRequestPriority::Visible,
    )
    .unwrap();

    let commands =
        registry.commands_for_request(&request, Path::new("/tmp/out.png"), ThumbnailSize::Normal);

    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].program(), "wild-thumb");

    let _ = fs::remove_dir_all(root);
}

#[test]
fn write_thumbnail_metadata_appends_freedesktop_text_chunks() {
    let root = temp_root("write-metadata");
    fs::create_dir_all(&root).unwrap();
    let thumbnail = root.join("thumb.png");
    fs::write(&thumbnail, test_thumbnail_png_without_metadata()).unwrap();

    write_thumbnail_metadata(&thumbnail, "file:///tmp/image.png", 42).unwrap();

    let metadata = thumbnail_metadata(&thumbnail).unwrap();
    assert_eq!(metadata.uri.as_deref(), Some("file:///tmp/image.png"));
    assert_eq!(metadata.mtime, Some(42));

    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn external_thumbnailer_generation_writes_normal_cache_with_metadata() {
    let root = temp_root("external-generate");
    fs::create_dir_all(&root).unwrap();
    let source = root.join("source.png");
    let fixture = root.join("fixture.png");
    let script = root.join("thumbnailer.sh");
    let thumbnailer_dir = root.join("thumbnailers");
    fs::write(&source, b"source").unwrap();
    fs::write(&fixture, test_thumbnail_png_without_metadata()).unwrap();
    write_executable_script(
        &script,
        format!("#!/bin/sh\n/bin/cp {} \"$2\"\n", sh_quote_path(&fixture)),
    );
    fs::create_dir_all(&thumbnailer_dir).unwrap();
    fs::write(
        thumbnailer_dir.join("fika.thumbnailer"),
        format!(
            "[Thumbnailer Entry]\nExec={} %i %o\nMimeType=image/png;\n",
            exec_quote_path(&script)
        ),
    )
    .unwrap();
    let registry = ThumbnailerRegistry::load_from_dirs([thumbnailer_dir]);
    let request = ThumbnailRequest::from_entry_metadata_with_mime(
        PaneId(1),
        Generation(1),
        ItemId(1),
        source,
        42,
        Some("image/png".to_string()),
        ThumbnailRequestPriority::Visible,
    )
    .unwrap();

    let hit = generate_thumbnail_with_external_thumbnailer_registry(&root, &request, &registry)
        .unwrap()
        .unwrap();

    let expected = thumbnail_cache_path(&root, ThumbnailSize::Normal, request.uri());
    assert_eq!(hit.size(), ThumbnailSize::Normal);
    assert_eq!(hit.path(), expected.as_path());
    let metadata = thumbnail_metadata(&expected).unwrap();
    assert_eq!(metadata.uri.as_deref(), Some(request.uri()));
    assert_eq!(metadata.mtime, Some(42));
    assert!(!thumbnail_failure_is_cached(
        &root,
        request.uri(),
        request.modified_secs()
    ));

    let _ = fs::remove_dir_all(root);
}

#[cfg(target_os = "linux")]
#[test]
fn external_thumbnailer_runs_with_suppressed_stdio() {
    let root = temp_root("external-stdio");
    fs::create_dir_all(&root).unwrap();
    let source = root.join("source.png");
    let fixture = root.join("fixture.png");
    let script = root.join("thumbnailer.sh");
    let thumbnailer_dir = root.join("thumbnailers");
    fs::write(&source, b"source").unwrap();
    fs::write(&fixture, test_thumbnail_png_without_metadata()).unwrap();
    write_executable_script(
        &script,
        format!(
            "#!/bin/sh\n\
             [ \"$(readlink /proc/$$/fd/1)\" = /dev/null ] || exit 11\n\
             [ \"$(readlink /proc/$$/fd/2)\" = /dev/null ] || exit 12\n\
             echo hidden stdout\n\
             echo hidden stderr >&2\n\
             /bin/cp {} \"$2\"\n",
            sh_quote_path(&fixture)
        ),
    );
    fs::create_dir_all(&thumbnailer_dir).unwrap();
    fs::write(
        thumbnailer_dir.join("fika.thumbnailer"),
        format!(
            "[Thumbnailer Entry]\nExec={} %i %o\nMimeType=image/png;\n",
            exec_quote_path(&script)
        ),
    )
    .unwrap();
    let registry = ThumbnailerRegistry::load_from_dirs([thumbnailer_dir]);
    let request = ThumbnailRequest::from_entry_metadata_with_mime(
        PaneId(1),
        Generation(1),
        ItemId(1),
        source,
        42,
        Some("image/png".to_string()),
        ThumbnailRequestPriority::Visible,
    )
    .unwrap();

    let hit = generate_thumbnail_with_external_thumbnailer_registry(&root, &request, &registry)
        .unwrap()
        .unwrap();

    assert_eq!(
        hit.path(),
        thumbnail_cache_path(&root, ThumbnailSize::Normal, request.uri()).as_path()
    );
    assert!(!thumbnail_failure_is_cached(
        &root,
        request.uri(),
        request.modified_secs()
    ));

    let _ = fs::remove_dir_all(root);
}

#[cfg(unix)]
#[test]
fn external_thumbnailer_failure_records_marker_and_skips_retry() {
    let root = temp_root("external-failure");
    fs::create_dir_all(&root).unwrap();
    let source = root.join("broken.png");
    let script = root.join("thumbnailer.sh");
    let attempts = root.join("attempts.txt");
    let thumbnailer_dir = root.join("thumbnailers");
    fs::write(&source, b"broken").unwrap();
    write_executable_script(
        &script,
        format!(
            "#!/bin/sh\necho attempt >> {}\nexit 2\n",
            sh_quote_path(&attempts)
        ),
    );
    fs::create_dir_all(&thumbnailer_dir).unwrap();
    fs::write(
        thumbnailer_dir.join("fika.thumbnailer"),
        format!(
            "[Thumbnailer Entry]\nExec={} %i %o\nMimeType=image/png;\n",
            exec_quote_path(&script)
        ),
    )
    .unwrap();
    let registry = ThumbnailerRegistry::load_from_dirs([thumbnailer_dir]);
    let request = ThumbnailRequest::from_entry_metadata_with_mime(
        PaneId(1),
        Generation(1),
        ItemId(1),
        source,
        42,
        Some("image/png".to_string()),
        ThumbnailRequestPriority::Visible,
    )
    .unwrap();

    assert!(
        generate_thumbnail_with_external_thumbnailer_registry(&root, &request, &registry)
            .unwrap()
            .is_none()
    );
    assert_eq!(fs::read_to_string(&attempts).unwrap(), "attempt\n");
    assert!(thumbnail_failure_is_cached(
        &root,
        request.uri(),
        request.modified_secs()
    ));
    assert!(!thumbnail_cache_path(&root, ThumbnailSize::Normal, request.uri()).is_file());
    let metadata = thumbnail_metadata(&thumbnail_failure_path(&root, request.uri())).unwrap();
    assert_eq!(metadata.uri.as_deref(), Some(request.uri()));
    assert_eq!(metadata.mtime, Some(42));

    assert!(
        generate_thumbnail_with_external_thumbnailer_registry(&root, &request, &registry)
            .unwrap()
            .is_none()
    );
    assert_eq!(fs::read_to_string(&attempts).unwrap(), "attempt\n");

    let _ = fs::remove_dir_all(root);
}

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

fn temp_root(name: &str) -> PathBuf {
    let root = env::temp_dir().join(format!("fika-thumbnail-{name}-{}", process::id()));
    let _ = fs::remove_dir_all(&root);
    root
}

fn test_request(
    root: &Path,
    name: &str,
    item_id: ItemId,
    generation: Generation,
    priority: ThumbnailRequestPriority,
) -> ThumbnailRequest {
    ThumbnailRequest::new(
        PaneId(1),
        generation,
        item_id,
        write_source(root, name),
        priority,
    )
    .unwrap()
}

fn write_source(root: &Path, name: &str) -> PathBuf {
    let path = root.join(name);
    fs::write(&path, b"source").unwrap();
    path
}

#[cfg(unix)]
fn write_executable_script(path: &Path, contents: String) {
    fs::write(path, contents).unwrap();
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

#[cfg(unix)]
fn exec_quote_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    format!("\"{}\"", path.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(unix)]
fn sh_quote_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    format!("'{}'", path.replace('\'', "'\\''"))
}

pub(crate) fn test_thumbnail_png(uri: &str, mtime: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend(PNG_SIGNATURE);
    bytes.extend(test_png_chunk(b"IHDR", &[0; 13]));
    bytes.extend(test_png_text_chunk("Thumb::URI", uri));
    bytes.extend(test_png_text_chunk("Thumb::MTime", &mtime.to_string()));
    bytes.extend(test_png_chunk(b"IEND", &[]));
    bytes
}

fn test_thumbnail_png_without_metadata() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend(PNG_SIGNATURE);
    bytes.extend(test_png_chunk(b"IHDR", &[0; 13]));
    bytes.extend(test_png_chunk(b"IDAT", FAILURE_THUMBNAIL_IDAT));
    bytes.extend(test_png_chunk(b"IEND", &[]));
    bytes
}

fn test_png_text_chunk(key: &str, value: &str) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend(key.as_bytes());
    data.push(0);
    data.extend(value.as_bytes());
    test_png_chunk(b"tEXt", &data)
}

fn test_png_chunk(chunk_type: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut chunk = Vec::new();
    chunk.extend((data.len() as u32).to_be_bytes());
    chunk.extend(chunk_type);
    chunk.extend(data);
    chunk.extend([0; 4]);
    chunk
}
