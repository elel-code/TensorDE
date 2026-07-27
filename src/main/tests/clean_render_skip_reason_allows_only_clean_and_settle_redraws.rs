
    #[test]
    fn clean_render_skip_reason_allows_only_clean_and_settle_redraws() {
        assert!(clean_render_skip_reason_allowed("redraw", false));
        assert!(clean_render_skip_reason_allowed("switch-redraw", true));
        assert!(!clean_render_skip_reason_allowed("redraw", true));
        assert!(!clean_render_skip_reason_allowed("zoom", true));
        assert!(!clean_render_skip_reason_allowed("wheel-scroll", true));
    }

    #[test]
    fn active_animation_redraws_are_deadline_paced() {
        let now = Instant::now();
        let next = now + Duration::from_millis(16);
        let mut scheduled = None;

        assert!(schedule_animation_redraw(
            true,
            &mut scheduled,
            now,
            Some(next)
        ));
        assert_eq!(scheduled, Some(next));
        assert!(!schedule_animation_redraw(
            true,
            &mut scheduled,
            now + Duration::from_millis(1),
            Some(next + Duration::from_millis(16))
        ));
        assert!(schedule_animation_redraw(
            true,
            &mut scheduled,
            next,
            Some(next + Duration::from_millis(16))
        ));
        assert!(!schedule_animation_redraw(
            false,
            &mut scheduled,
            next,
            None
        ));
        assert!(scheduled.is_none());
    }

    #[test]
    fn skipped_clean_render_consumes_pending_redraw_without_presenting() {
        assert!(ShellRenderOutcome::SkippedClean.consumed_redraw_request());
        assert!(!ShellRenderOutcome::SkippedClean.presented());
        assert!(ShellRenderOutcome::Presented.consumed_redraw_request());
        assert!(ShellRenderOutcome::Presented.presented());
        assert!(!ShellRenderOutcome::NotReady.consumed_redraw_request());
        assert!(!ShellRenderOutcome::NotReady.presented());
    }

    #[test]
    fn vertex_upload_hash_skips_unchanged_vertices() {
        let first = [
            QuadVertex {
                position: [0.0, 0.0],
                color: [1.0, 0.0, 0.0, 1.0],
            },
            QuadVertex {
                position: [1.0, 1.0],
                color: [0.0, 1.0, 0.0, 1.0],
            },
        ];
        let second = [
            first[0],
            QuadVertex {
                position: [2.0, 1.0],
                color: [0.0, 1.0, 0.0, 1.0],
            },
        ];
        let mut last_hash = None;

        assert_eq!(
            upload_vertex_hash_for_test(&first, &mut last_hash),
            VertexBufferUploadStats {
                writes: 1,
                skips: 0
            }
        );
        assert_eq!(
            upload_vertex_hash_for_test(&first, &mut last_hash),
            VertexBufferUploadStats {
                writes: 0,
                skips: 1
            }
        );
        assert_eq!(
            upload_vertex_hash_for_test(&second, &mut last_hash),
            VertexBufferUploadStats {
                writes: 1,
                skips: 0
            }
        );
    }

    #[test]
    fn icon_gpu_identity_is_size_free_for_roles_and_theme() {
        // MIME role: not bound to a filesystem path or size bucket.
        let mime = FileIconKind::Mime {
            mime: Arc::from("application/x-executable"),
        };
        let a = IconGpuUploadKey::role(mime.clone());
        let b = IconGpuUploadKey::role(mime);
        assert_eq!(a, b);

        // Theme asset: path only — zoom must not allocate a new GPU slot.
        let theme_48 = IconGpuUploadKey::theme_asset(PathBuf::from("/icons/app.png"));
        let theme_96 = IconGpuUploadKey::theme_asset(PathBuf::from("/icons/app.png"));
        assert_eq!(theme_48, theme_96);
        assert_ne!(
            theme_48,
            IconGpuUploadKey::theme_asset(PathBuf::from("/icons/other.png"))
        );

        // Content previews: path + mtime (still size-free).
        let c1 = IconGpuUploadKey::content(PathBuf::from("/photos/a.jpg"), 7);
        let c2 = IconGpuUploadKey::content(PathBuf::from("/photos/a.jpg"), 7);
        let c3 = IconGpuUploadKey::content(PathBuf::from("/photos/a.jpg"), 8);
        assert_eq!(c1, c2);
        assert_ne!(c1, c3);

        // Role vs content must never collide.
        assert_ne!(a, c1);
    }

    #[test]
    fn icon_gpu_slot_content_hash_changes_when_pixels_change() {
        let identity = IconGpuUploadKey::role(FileIconKind::Directory);
        let a = padded_icon_atlas_raster(&test_icon_raster(16, 7));
        let b = padded_icon_atlas_raster(&test_icon_raster(16, 9));
        let hash_a = hash_bytes_with_len(a.pixels.as_ref());
        let hash_b = hash_bytes_with_len(b.pixels.as_ref());
        assert_ne!(hash_a, hash_b);
        let slot_a = IconGpuSlot {
            identity: identity.clone(),
            width: a.width,
            height: a.height,
            content_width: 16,
            content_height: 16,
            content_hash: hash_a,
            upload: Some(a),
            dmabuf: None,
        };
        let slot_b = IconGpuSlot {
            identity,
            width: b.width,
            height: b.height,
            content_width: 16,
            content_height: 16,
            content_hash: hash_b,
            upload: Some(b),
            dmabuf: None,
        };
        // Same GPU identity, different generation → in-place rewrite, not new key.
        assert_eq!(slot_a.identity, slot_b.identity);
        assert_ne!(slot_a.content_hash, slot_b.content_hash);
    }

    #[test]
    fn thumbnail_candidates_are_projected_from_visible_previewable_files() {
        let scene = test_scene(
            vec![
                test_entry_with_mime_and_modified("photo.png", false, "image/png", Some(42)),
                test_entry_with_mime_and_modified("notes.txt", false, "text/plain", Some(42)),
                test_entry("folder", true),
            ],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(700, 320);
        let projection = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();

        assert_eq!(
            scene.thumbnail_candidate_count_for_projection(&projection),
            1
        );
    }

    #[test]
    fn folder_preview_candidates_are_projected_from_local_directories_with_metadata() {
        let scene = test_scene(
            vec![
                test_entry_with_mime_and_modified("photo.png", false, "image/png", Some(42)),
                test_entry_with_mime_and_modified("album", true, "inode/directory", Some(7)),
            ],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(700, 320);
        let projection = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();

        assert_eq!(
            scene.thumbnail_candidate_count_for_projection(&projection),
            1
        );
        assert_eq!(
            scene.folder_preview_role_candidate_count_for_projection(&projection),
            1
        );
    }

    #[test]
    fn folder_preview_candidate_uses_directory_modified_time_for_role_request() {
        let root = test_dir("directory-candidate-stamp");
        let album = root.join("album");
        fs::create_dir_all(&album).unwrap();
        fs::write(album.join("cover.png"), b"preview").unwrap();
        let mut scene = test_scene(
            vec![test_entry_with_mime_and_modified(
                "album",
                true,
                "inode/directory",
                Some(7),
            )],
            ShellViewMode::Icons,
        );
        scene.panes[ShellPaneId::SLOT_0].path = root.clone();
        let projection = scene
            .pane_projection(ShellPaneId::SLOT_0, PhysicalSize::new(700, 320))
            .unwrap();

        let request = scene
            .folder_preview_role_request_for_pane_entry(
                projection.view,
                0,
                48,
                ThumbnailRequestPriority::Visible,
            )
            .unwrap();

        assert_eq!(request.key.directory_modified_secs, 7);
        assert_eq!(request.key.path, album);
        assert_ne!(folder_preview_thumbnail_stamp(&album, 7), 7);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn folder_preview_role_request_size_matches_render_lookup_size() {
        let root = test_dir("directory-preview-size-key");
        let album = root.join("album");
        fs::create_dir_all(&album).unwrap();
        fs::write(album.join("cover.png"), b"preview").unwrap();
        let scene = test_scene(
            vec![test_entry_with_mime_and_modified(
                "album",
                true,
                "inode/directory",
                Some(7),
            )],
            ShellViewMode::Icons,
        );
        let mut scene = scene;
        scene.panes[ShellPaneId::SLOT_0].path = root.clone();
        let projection = scene
            .pane_projection(ShellPaneId::SLOT_0, PhysicalSize::new(700, 320))
            .unwrap();
        let item = projection.visible_items[0];
        let entry_index = projection.view.filtered_indexes[item.layout.model_index];
        let pixmap_layout =
            ItemPixmapLayout::from_item_layout(projection.view.view_mode, item.layout);
        let expected_size = scene.folder_preview_role_size_px_for_item(pixmap_layout);
        assert_eq!(expected_size, 128);
        let request = scene
            .folder_preview_role_request_for_pane_entry(
                projection.view,
                entry_index,
                expected_size,
                ThumbnailRequestPriority::Visible,
            )
            .unwrap();

        assert_eq!(request.key.size_px, expected_size);
        scene.folder_preview_roles.borrow_mut().insert_ready(
            request.key,
            FolderPreviewReady {
                stamp: 11,
                size_px: expected_size,
                raster: test_icon_raster(2, 3),
            },
        );
        assert!(
            scene
                .folder_preview_role_for_pane_entry(projection.view, entry_index, pixmap_layout)
                .is_some()
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn folder_preview_role_runs_in_compact_below_32px_icon_size_like_dolphin() {
        let root = test_dir("directory-preview-compact-small-icon");
        let album = root.join("album");
        fs::create_dir_all(&album).unwrap();
        fs::write(album.join("cover.png"), b"preview").unwrap();
        let mut scene = test_scene(
            vec![test_entry_with_mime_and_modified(
                "album",
                true,
                "inode/directory",
                Some(7),
            )],
            ShellViewMode::Compact,
        );
        scene.panes[ShellPaneId::SLOT_0].path = root.clone();
        let projection = scene
            .pane_projection(ShellPaneId::SLOT_0, PhysicalSize::new(700, 320))
            .unwrap();
        let item = projection.visible_items[0];
        assert!(
            item.layout
                .icon_rect
                .width
                .max(item.layout.icon_rect.height)
                < 32.0
        );
        let entry_index = projection.view.filtered_indexes[item.layout.model_index];
        let pixmap_layout =
            ItemPixmapLayout::from_item_layout(projection.view.view_mode, item.layout);
        let expected_size = scene.folder_preview_role_size_px_for_item(pixmap_layout);
        assert_eq!(expected_size, 128);

        let stats = scene.update_folder_preview_roles_for_projections(std::slice::from_ref(
            &projection,
        ));
        assert_eq!(stats.visible, 1);
        assert_eq!(stats.queued, 1);

        scene.folder_preview_roles.borrow_mut().insert_ready(
            FolderPreviewRoleKey::new(album, 7, expected_size),
            FolderPreviewReady {
                stamp: 11,
                size_px: expected_size,
                raster: test_icon_raster(28, 20),
            },
        );

        assert!(
            scene
                .folder_preview_role_for_pane_entry(projection.view, entry_index, pixmap_layout)
                .is_some()
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn folder_preview_role_cache_size_matches_dolphin_preview_job_cache_size() {
        assert_eq!(folder_preview_role_cache_size(16.0), 128);
        assert_eq!(folder_preview_role_cache_size(128.0), 128);
        assert_eq!(folder_preview_role_cache_size(128.1), 256);
        assert_eq!(folder_preview_role_cache_size(256.0), 256);
    }

    #[test]
    fn folder_preview_role_uses_closest_ready_size_while_zoom_request_is_pending() {
        let root = test_dir("directory-preview-closest-size");
        let album = root.join("album");
        fs::create_dir_all(&album).unwrap();
        fs::write(album.join("cover.png"), b"preview").unwrap();
        let mut scene = test_scene(
            vec![test_entry_with_mime_and_modified(
                "album",
                true,
                "inode/directory",
                Some(7),
            )],
            ShellViewMode::Icons,
        );
        scene.panes[ShellPaneId::SLOT_0].path = root.clone();
        scene.folder_preview_roles.borrow_mut().insert_ready(
            FolderPreviewRoleKey::new(album, 7, 48),
            FolderPreviewReady {
                stamp: 11,
                size_px: 48,
                raster: test_icon_raster(32, 24),
            },
        );
        let projection = scene
            .pane_projection(ShellPaneId::SLOT_0, PhysicalSize::new(700, 320))
            .unwrap();
        let entry_index = projection.view.filtered_indexes[0];
        let pixmap_layout = ItemPixmapLayout {
            view_mode: ShellViewMode::Icons,
            icon_rect: ViewRect {
                x: 16.0,
                y: 2.0,
                width: 128.0,
                height: 128.0,
            },
            text_rect: ViewRect {
                x: 2.0,
                y: 132.0,
                width: 156.0,
                height: 18.0,
            },
            text_midline_shift: 0.0,
        };
        let preview = scene
            .folder_preview_role_for_pane_entry(projection.view, entry_index, pixmap_layout)
            .unwrap();

        assert_eq!(preview.size_px, 48);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn zoom_clears_folder_preview_request_lifecycle_but_keeps_ready_pixmap() {
        let root = test_dir("directory-preview-zoom-lifecycle");
        let album = root.join("album");
        fs::create_dir_all(&album).unwrap();
        let mut scene = test_scene(
            vec![test_entry_with_mime_and_modified(
                "album",
                true,
                "inode/directory",
                Some(7),
            )],
            ShellViewMode::Icons,
        );
        scene.panes[ShellPaneId::SLOT_0].path = root.clone();
        let old_key = FolderPreviewRoleKey::new(album.clone(), 7, 48);
        let new_key = FolderPreviewRoleKey::new(album.clone(), 7, 96);
        {
            let mut roles = scene.folder_preview_roles.borrow_mut();
            roles.insert_ready(
                old_key.clone(),
                FolderPreviewReady {
                    stamp: 11,
                    size_px: 48,
                    raster: test_icon_raster(2, 3),
                },
            );
            roles.failed.insert(new_key.clone());
            roles.finished.insert(new_key.clone());
            roles
                .pending
                .insert(new_key.clone(), ThumbnailRequestPriority::Visible);
            roles.active.insert(new_key.clone());
        }

        assert!(scene.zoom(ZoomAction::In, PhysicalSize::new(700, 320)));

        let roles = scene.folder_preview_roles.borrow();
        assert!(roles.ready.contains_key(&old_key));
        assert!(roles.failed.is_empty());
        assert!(roles.finished.is_empty());
        assert!(roles.pending.is_empty());
        assert!(roles.active.is_empty());
        assert!(roles.preview_or_closest(&album, 7, 96).is_some());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn folder_preview_role_rejects_late_old_size_result_after_zoom_size_changes() {
        let root = PathBuf::from("/tmp/fika-directory-preview-late-zoom-result");
        let old_key = FolderPreviewRoleKey::new(root.clone(), 7, 48);
        let new_key = FolderPreviewRoleKey::new(root.clone(), 7, 96);
        let (request_tx, _request_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let mut runtime = ShellFolderPreviewRoleRuntime {
            ready: HashMap::new(),
            failed: HashSet::new(),
            pending: HashMap::from([(old_key.clone(), ThumbnailRequestPriority::Visible)]),
            finished: HashSet::new(),
            active: HashSet::from([new_key]),
            frame: 0,
            ready_bytes: 0,
            ready_max_bytes: THUMBNAIL_READY_CACHE_MAX_BYTES,
            request_tx: Some(request_tx),
            result_rx,
        };
        result_tx
            .send(FolderPreviewRoleResult {
                key: old_key.clone(),
                preview: Some(FolderPreviewReady {
                    stamp: 11,
                    size_px: 48,
                    raster: test_icon_raster(2, 3),
                }),
            })
            .unwrap();

        let stats = runtime.drain_results();

        assert_eq!(stats.results, 1);
        assert_eq!(stats.applied, 0);
        assert!(stats.changes.is_empty());
        assert!(!runtime.ready.contains_key(&old_key));
        assert!(runtime.preview_or_closest(&root, 7, 96).is_none());
    }
