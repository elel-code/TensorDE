    #[test]
    fn thumbnail_read_ahead_indexes_follow_file_manager_order() {
        let indexes = shell_file_manager_read_ahead_indexes(4..7, 16, 3);

        assert_eq!(&indexes[..6], &[7, 8, 9, 10, 11, 12]);
        assert!(!indexes.iter().any(|index| (4..7).contains(index)));
        assert_eq!(
            indexes.iter().copied().collect::<BTreeSet<_>>().len(),
            indexes.len()
        );
    }

    #[test]
    fn priority_worker_queue_promotes_visible_request_over_deferred() {
        let mut queue = PriorityWorkerQueue::default();
        let first = test_thumbnail_source_request("first.png", ThumbnailRequestPriority::Deferred);
        let second =
            test_thumbnail_source_request("second.png", ThumbnailRequestPriority::Deferred);
        let promoted =
            test_thumbnail_source_request("first.png", ThumbnailRequestPriority::Visible);

        queue.push(first);
        queue.push(second.clone());
        queue.push(promoted.clone());

        let next = queue.pop_ready().unwrap();
        assert_eq!(next.key, promoted.key);
        assert_eq!(next.priority, ThumbnailRequestPriority::Visible);

        let next = queue.pop_ready().unwrap();
        assert_eq!(next.key, second.key);
        assert_eq!(next.priority, ThumbnailRequestPriority::Deferred);

        assert!(queue.pop_ready().is_none());
    }

    #[test]
    fn folder_preview_source_chooses_first_visible_previewable_file() {
        let root = test_dir("directory-preview-source");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("notes.txt"), b"notes").unwrap();
        fs::write(root.join(".hidden.png"), b"hidden").unwrap();
        fs::write(root.join("zeta.jpg"), b"zeta").unwrap();
        fs::write(root.join("Cover.PNG"), b"cover").unwrap();

        let source = folder_preview_thumbnail_source(&root).unwrap();

        assert_eq!(source.path, root.join("Cover.PNG"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn folder_preview_source_uses_magic_for_extensionless_images() {
        let root = test_dir("directory-preview-source-magic");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("cover"), b"\x89PNG\r\n\x1a\npreview").unwrap();

        let source = folder_preview_thumbnail_source(&root).unwrap();

        assert_eq!(source.path, root.join("cover"));
        assert_eq!(source.mime_type.as_deref(), Some("image/png"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn folder_preview_source_ignores_video_without_preview_support() {
        let root = test_dir("directory-preview-source-video");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("01.mp4"), b"\0\0\0\x18ftypmp42preview").unwrap();
        image::RgbImage::from_pixel(8, 4, image::Rgb([210, 40, 80]))
            .save(root.join("安炳琨-20230557126.jpeg"))
            .unwrap();

        let sources = folder_preview_thumbnail_sources(&root);

        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].path, root.join("安炳琨-20230557126.jpeg"));
        assert_eq!(sources[0].mime_type.as_deref(), Some("image/jpeg"));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn folder_preview_source_includes_windows_executable_icons() {
        let root = test_dir("directory-preview-source-exe");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("notes.txt"), b"notes").unwrap();
        fs::write(root.join("setup.exe"), b"MZpreview").unwrap();

        let source = folder_preview_thumbnail_source(&root).unwrap();

        assert_eq!(source.path, root.join("setup.exe"));
        assert!(
            matches!(
                source.mime_type.as_deref(),
                Some(
                    "application/vnd.microsoft.portable-executable"
                        | "application/x-msdownload"
                        | "application/x-ms-dos-executable"
                )
            ),
            "unexpected MIME: {:?}",
            source.mime_type
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn folder_preview_sources_choose_first_four_visible_previewable_files() {
        let root = test_dir("directory-preview-sources");
        fs::create_dir_all(&root).unwrap();
        for name in [
            "notes.txt",
            ".hidden.png",
            "05.png",
            "02.png",
            "04.png",
            "01.png",
            "03.png",
        ] {
            fs::write(root.join(name), b"preview").unwrap();
        }

        let names = folder_preview_thumbnail_sources(&root)
            .into_iter()
            .map(|source| {
                source
                    .path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect::<Vec<_>>();

        assert_eq!(names, vec!["01.png", "02.png", "03.png", "04.png"]);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn folder_preview_stamp_tracks_selected_child_sources() {
        let root = test_dir("directory-preview-stamp");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("b.png"), b"preview").unwrap();

        let first = folder_preview_thumbnail_stamp(&root, 7);

        fs::write(root.join("a.png"), b"preview").unwrap();
        let second = folder_preview_thumbnail_stamp(&root, 7);

        assert_ne!(first, second);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn folder_preview_stamp_falls_back_to_directory_modified_without_sources() {
        let root = test_dir("directory-preview-empty-stamp");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("notes.txt"), b"notes").unwrap();

        assert_eq!(folder_preview_thumbnail_stamp(&root, 7), 7);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn folder_preview_role_metadata_uses_child_aware_stamp() {
        let root = test_dir("directory-preview-metadata");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("cover.png"), b"preview").unwrap();

        let metadata = folder_preview_role_metadata_for_path(&root, 7).unwrap();

        assert_eq!(metadata.sources.len(), 1);
        assert_eq!(metadata.sources[0].path, root.join("cover.png"));
        assert_eq!(
            metadata.stamp,
            folder_preview_thumbnail_stamp_from_sources(7, &metadata.sources)
        );
        assert_ne!(metadata.stamp, 7);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn folder_preview_role_failed_item_is_finished_until_identity_changes() {
        let root = PathBuf::from("/tmp/fika-directory-preview-recent-failure");
        let key = FolderPreviewRoleKey::new(root.clone(), 7, 48);
        let (request_tx, request_rx) = mpsc::channel();
        let (_result_tx, result_rx) = mpsc::channel();
        let mut runtime = ShellFolderPreviewRoleRuntime {
            ready: HashMap::new(),
            failed: HashSet::from([key.clone()]),
            pending: HashMap::new(),
            finished: HashSet::from([key.clone()]),
            active: HashSet::from([key.clone()]),
            frame: 0,
            ready_bytes: 0,
            ready_max_bytes: THUMBNAIL_READY_CACHE_MAX_BYTES,
            request_tx: Some(request_tx),
            result_rx,
        };
        assert!(!runtime.has_visible_pending());

        let stats = runtime.queue_candidates([FolderPreviewRoleRequest {
            key,
            priority: ThumbnailRequestPriority::Visible,
        }]);
        assert_eq!(stats.failed, 1);
        assert_eq!(stats.queued, 0);
        assert!(request_rx.try_recv().is_err());
    }

    #[test]
    fn folder_preview_role_failure_result_marks_empty_icon_pixmap_finished() {
        let root = PathBuf::from("/tmp/fika-directory-preview-failure-result");
        let key = FolderPreviewRoleKey::new(root.clone(), 7, 48);
        let (request_tx, request_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let mut runtime = ShellFolderPreviewRoleRuntime {
            ready: HashMap::new(),
            failed: HashSet::new(),
            pending: HashMap::from([(key.clone(), ThumbnailRequestPriority::Visible)]),
            finished: HashSet::new(),
            active: HashSet::from([key.clone()]),
            frame: 0,
            ready_bytes: 0,
            ready_max_bytes: THUMBNAIL_READY_CACHE_MAX_BYTES,
            request_tx: Some(request_tx),
            result_rx,
        };
        result_tx
            .send(FolderPreviewRoleResult {
                key: key.clone(),
                preview: None,
            })
            .unwrap();

        let result_stats = runtime.drain_results();
        assert_eq!(result_stats.results, 1);
        assert_eq!(result_stats.applied, 1);
        assert_eq!(result_stats.changes.len(), 1);
        assert_eq!(result_stats.changes[0].key, key);
        assert!(result_stats.changes[0].previous.is_none());
        assert!(runtime.failed.contains(&key));
        assert!(runtime.finished.contains(&key));
        assert!(!runtime.has_pending());

        let queue_stats = runtime.queue_candidates([FolderPreviewRoleRequest {
            key,
            priority: ThumbnailRequestPriority::Visible,
        }]);
        assert_eq!(queue_stats.failed, 1);
        assert_eq!(queue_stats.queued, 0);
        assert!(request_rx.try_recv().is_err());
    }

    #[test]
    fn folder_preview_role_visible_candidate_promotes_deferred_work() {
        let root = PathBuf::from("/tmp/fika-directory-preview-promote");
        let key = FolderPreviewRoleKey::new(root.clone(), 7, 48);
        let (request_tx, request_rx) = mpsc::channel();
        let (_result_tx, result_rx) = mpsc::channel();
        let mut runtime = ShellFolderPreviewRoleRuntime {
            ready: HashMap::new(),
            failed: HashSet::new(),
            pending: HashMap::from([(key.clone(), ThumbnailRequestPriority::Deferred)]),
            finished: HashSet::new(),
            active: HashSet::from([key.clone()]),
            frame: 0,
            ready_bytes: 0,
            ready_max_bytes: THUMBNAIL_READY_CACHE_MAX_BYTES,
            request_tx: Some(request_tx),
            result_rx,
        };
        assert!(!runtime.has_visible_pending());

        let stats = runtime.queue_candidates([FolderPreviewRoleRequest {
            key: key.clone(),
            priority: ThumbnailRequestPriority::Visible,
        }]);
        assert_eq!(stats.visible, 1);
        assert_eq!(stats.queued, 1);
        let request = request_rx.try_recv().unwrap();
        assert_eq!(request.key, key);
        assert_eq!(request.priority, ThumbnailRequestPriority::Visible);
        assert!(runtime.has_visible_pending());
    }

    #[test]
    fn folder_preview_role_pending_visible_candidate_does_not_requeue_on_hover_redraw() {
        let root = PathBuf::from("/tmp/fika-directory-preview-hover-redraw");
        let key = FolderPreviewRoleKey::new(root.clone(), 7, 48);
        let (request_tx, request_rx) = mpsc::channel();
        let (_result_tx, result_rx) = mpsc::channel();
        let mut runtime = ShellFolderPreviewRoleRuntime {
            ready: HashMap::new(),
            failed: HashSet::new(),
            pending: HashMap::from([(key.clone(), ThumbnailRequestPriority::Visible)]),
            finished: HashSet::new(),
            active: HashSet::from([key.clone()]),
            frame: 0,
            ready_bytes: 0,
            ready_max_bytes: THUMBNAIL_READY_CACHE_MAX_BYTES,
            request_tx: Some(request_tx),
            result_rx,
        };

        let stats = runtime.queue_candidates([FolderPreviewRoleRequest {
            key,
            priority: ThumbnailRequestPriority::Visible,
        }]);

        assert_eq!(stats.visible, 1);
        assert_eq!(stats.queued, 0);
        assert!(request_rx.try_recv().is_err());
    }

    #[test]
    fn folder_preview_role_has_pending_does_not_apply_late_results_after_dirty_key() {
        let root = PathBuf::from("/tmp/fika-directory-preview-late-result");
        let key = FolderPreviewRoleKey::new(root.clone(), 7, 48);
        let (request_tx, _request_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let mut runtime = ShellFolderPreviewRoleRuntime {
            ready: HashMap::new(),
            failed: HashSet::new(),
            pending: HashMap::from([(key.clone(), ThumbnailRequestPriority::Visible)]),
            finished: HashSet::new(),
            active: HashSet::from([key.clone()]),
            frame: 0,
            ready_bytes: 0,
            ready_max_bytes: THUMBNAIL_READY_CACHE_MAX_BYTES,
            request_tx: Some(request_tx),
            result_rx,
        };
        result_tx
            .send(FolderPreviewRoleResult {
                key: key.clone(),
                preview: Some(FolderPreviewReady {
                    stamp: 11,
                    size_px: 48,
                    source: test_folder_preview_source(test_gpu_icon_spec(2, 3)),
                }),
            })
            .unwrap();

        assert!(runtime.has_pending());
        assert!(runtime.ready.is_empty());

        let stats = runtime.drain_results();
        assert_eq!(stats.applied, 1);
        assert!(runtime.ready.contains_key(&key));
        assert!(!runtime.has_pending());
    }

    #[test]
    fn folder_preview_role_projection_update_marks_active_before_drain() {
        let root = PathBuf::from("/tmp/fika-directory-preview-active-before-drain");
        let key = FolderPreviewRoleKey::new(root.clone(), 7, 48);
        let (request_tx, _request_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let mut runtime = ShellFolderPreviewRoleRuntime {
            ready: HashMap::new(),
            failed: HashSet::new(),
            pending: HashMap::from([(key.clone(), ThumbnailRequestPriority::Visible)]),
            finished: HashSet::new(),
            active: HashSet::new(),
            frame: 0,
            ready_bytes: 0,
            ready_max_bytes: THUMBNAIL_READY_CACHE_MAX_BYTES,
            request_tx: Some(request_tx),
            result_rx,
        };
        result_tx
            .send(FolderPreviewRoleResult {
                key: key.clone(),
                preview: Some(FolderPreviewReady {
                    stamp: 11,
                    size_px: 48,
                    source: test_folder_preview_source(test_gpu_icon_spec(2, 3)),
                }),
            })
            .unwrap();

        runtime.queue_candidates([FolderPreviewRoleRequest {
            key: key.clone(),
            priority: ThumbnailRequestPriority::Visible,
        }]);
        let stats = runtime.drain_results();

        assert_eq!(stats.applied, 1);
        assert!(runtime.ready.contains_key(&key));
    }

    #[test]
    fn folder_preview_role_resolves_encoded_source_child_thumbnail_cache_hit() {
        let cache_root = test_dir("directory-preview-cache");
        let root = test_dir("directory-preview-worker");
        fs::create_dir_all(&root).unwrap();
        let cover = root.join("cover.png");
        fs::write(&cover, b"cover").unwrap();
        let cover_modified_secs = cover
            .metadata()
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let cover_uri = fika_core::thumbnail_uri_for_path(&cover).unwrap();
        let thumbnail = fika_core::thumbnail_cache_path(
            &cache_root,
            fika_core::ThumbnailSize::Normal,
            &cover_uri,
        );
        write_test_thumbnail_png(&thumbnail, &cover_uri, cover_modified_secs);
        let directory_modified_secs = root
            .metadata()
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let request = FolderPreviewRoleRequest {
            key: FolderPreviewRoleKey::new(root.clone(), directory_modified_secs, 48),
            priority: ThumbnailRequestPriority::Visible,
        };

        let preview =
            folder_preview_for_request(&cache_root, &ThumbnailerRegistry::default(), &request)
                .unwrap();

        assert_eq!(preview.source.size_px(), 48);
        assert_eq!(preview.source.folder_preview_children(), Some([thumbnail].as_slice()));

        let _ = fs::remove_dir_all(cache_root);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn folder_preview_role_resolves_encoded_source_local_image_when_thumbnailer_has_no_result() {
        let cache_root = test_dir("directory-preview-direct-image-cache");
        let root = test_dir("directory-preview-direct-image-worker");
        fs::create_dir_all(&root).unwrap();
        let cover = root.join("cover.png");
        image::RgbaImage::from_pixel(8, 4, image::Rgba([210, 40, 80, 255]))
            .save(&cover)
            .unwrap();
        let directory_modified_secs = root
            .metadata()
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let request = FolderPreviewRoleRequest {
            key: FolderPreviewRoleKey::new(root.clone(), directory_modified_secs, 48),
            priority: ThumbnailRequestPriority::Visible,
        };

        let preview =
            folder_preview_for_request(&cache_root, &ThumbnailerRegistry::default(), &request)
                .unwrap();

        assert_eq!(preview.source.size_px(), 48);
        assert_eq!(preview.source.folder_preview_children(), Some([cover].as_slice()));

        let _ = fs::remove_dir_all(cache_root);
        let _ = fs::remove_dir_all(root);
    }
