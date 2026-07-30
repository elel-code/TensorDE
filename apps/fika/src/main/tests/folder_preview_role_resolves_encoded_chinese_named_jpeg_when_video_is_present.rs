
    #[test]
    fn folder_preview_role_resolves_encoded_chinese_named_jpeg_when_video_is_present() {
        let cache_root = test_dir("directory-preview-chinese-jpeg-cache");
        let root = test_dir("directory-preview-chinese-jpeg-worker");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("01.mp4"), b"\0\0\0\x18ftypmp42preview").unwrap();
        image::RgbImage::from_pixel(8, 4, image::Rgb([210, 40, 80]))
            .save(root.join("安炳琨-20230557126.jpeg"))
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
        assert_eq!(preview.source.folder_preview_children().unwrap().len(), 1);

        let _ = fs::remove_dir_all(cache_root);
        let _ = fs::remove_dir_all(root);
    }
    #[test]
    fn folder_preview_role_composes_cached_windows_executable_icon_thumbnail() {
        let cache_root = test_dir("directory-preview-exe-cache");
        let root = test_dir("directory-preview-exe-worker");
        fs::create_dir_all(&root).unwrap();
        let app = root.join("setup.exe");
        fs::write(&app, b"MZpreview").unwrap();
        let modified_secs = app
            .metadata()
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let uri = fika_core::thumbnail_uri_for_path(&app).unwrap();
        let thumbnail =
            fika_core::thumbnail_cache_path(&cache_root, fika_core::ThumbnailSize::Normal, &uri);
        write_test_thumbnail_png_with_color(&thumbnail, &uri, modified_secs, [90, 42, 210, 255]);
        let directory_modified_secs = root
            .metadata()
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let request = FolderPreviewRoleRequest {
            key: FolderPreviewRoleKey::new(root.clone(), directory_modified_secs, 64),
            priority: ThumbnailRequestPriority::Visible,
        };

        let preview =
            folder_preview_for_request(&cache_root, &ThumbnailerRegistry::default(), &request)
                .unwrap();

        assert_eq!(preview.source.size_px(), 64);
        assert_eq!(preview.source.folder_preview_children(), Some([thumbnail].as_slice()));

        let _ = fs::remove_dir_all(cache_root);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn folder_preview_role_composes_multiple_child_thumbnail_cache_hits() {
        let cache_root = test_dir("directory-preview-multi-cache");
        let root = test_dir("directory-preview-multi-worker");
        fs::create_dir_all(&root).unwrap();
        let children = [
            ("01.png", [224, 32, 32, 255]),
            ("02.png", [32, 224, 32, 255]),
            ("03.png", [32, 32, 224, 255]),
            ("04.png", [224, 224, 32, 255]),
        ];
        for (name, color) in children {
            let path = root.join(name);
            fs::write(&path, b"child").unwrap();
            let modified_secs = path
                .metadata()
                .unwrap()
                .modified()
                .unwrap()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs();
            let uri = fika_core::thumbnail_uri_for_path(&path).unwrap();
            let thumbnail = fika_core::thumbnail_cache_path(
                &cache_root,
                fika_core::ThumbnailSize::Normal,
                &uri,
            );
            write_test_thumbnail_png_with_color(&thumbnail, &uri, modified_secs, color);
        }
        let directory_modified_secs = root
            .metadata()
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let request = FolderPreviewRoleRequest {
            key: FolderPreviewRoleKey::new(root.clone(), directory_modified_secs, 64),
            priority: ThumbnailRequestPriority::Visible,
        };

        let preview =
            folder_preview_for_request(&cache_root, &ThumbnailerRegistry::default(), &request)
                .unwrap();

        assert_eq!(preview.source.size_px(), 64);
        assert_eq!(preview.source.folder_preview_children().unwrap().len(), 4);

        let _ = fs::remove_dir_all(cache_root);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn folder_preview_gpu_source_keeps_supplied_child_sources_encoded() {
        let cache_root = test_dir("directory-preview-supplied-cache");
        let root = test_dir("directory-preview-supplied-worker");
        let source_root = test_dir("directory-preview-supplied-source");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&source_root).unwrap();
        let cover = source_root.join("cover.png");
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
        let source = folder_preview_gpu_source_for_sources(
            &cache_root,
            &ThumbnailerRegistry::default(),
            &root,
            &[FolderPreviewThumbnailSource {
                path: cover,
                modified_secs: cover_modified_secs,
                mime_type: Some("image/png".to_string()),
            }],
            ThumbnailRequestPriority::Visible,
            48,
        )
        .unwrap();

        assert_eq!(source.size_px(), 48);
        assert_eq!(source.folder_preview_children(), Some([thumbnail].as_slice()));

        let _ = fs::remove_dir_all(cache_root);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(source_root);
    }

    #[test]
    fn directory_folder_preview_angles_are_stable_and_non_grid_like_file_manager() {
        let seed = folder_preview_directory_seed(Path::new("/home/yk/Documents/wallper"));
        let angles = (0..FILE_MANAGER_FOLDER_PREVIEW_MAX_IMAGES)
            .map(|index| folder_preview_thumbnail_angle(seed, index))
            .collect::<Vec<_>>();

        assert!(angles.iter().all(|angle| (-8..=8).contains(angle)));
        assert!(angles.iter().any(|angle| *angle != 0));
        assert!(angles.windows(2).any(|pair| pair[0] != pair[1]));
    }

        #[test]
    fn three_folder_preview_children_center_bottom_thumbnail() {
        let layout = FileManagerDirectoryPreviewLayout::new(128).unwrap();
        let slots = folder_preview_thumbnail_slots(3, layout);
        let available_width = layout
            .folder_size
            .saturating_sub(layout.left_margin + layout.right_margin);
        let centered_x =
            layout.left_margin + available_width.saturating_sub(layout.segment_width) / 2;

        assert_eq!(slots.len(), 3);
        assert_eq!(slots[0].x, layout.left_margin);
        assert_eq!(
            slots[1].x,
            layout.left_margin + layout.segment_width + layout.spacing
        );
        assert_eq!(slots[2].x, centered_x);
        assert_eq!(
            slots[2].y,
            layout.top_margin + layout.segment_height + layout.spacing
        );
    }

    #[test]
    fn thumbnail_ready_cache_evicts_old_read_ahead_results() {
        let cache_root = test_dir("thumbnail-ready-cache-root");
        let mut resolver = ThumbnailSourceResolver::with_cache_root(cache_root.clone());
        resolver.ready_max_bytes = usize::MAX;
        let first = ThumbnailSourceKey::thumbnail(PathBuf::from("/tmp/first.png"), 8, 1);
        let second = ThumbnailSourceKey::thumbnail(PathBuf::from("/tmp/second.png"), 8, 1);
        let third = ThumbnailSourceKey::thumbnail(PathBuf::from("/tmp/third.png"), 8, 1);

        let first_source = test_thumbnail_gpu_source("first.png", 8);
        let second_source = test_thumbnail_gpu_source("second.png", 8);
        let third_source = test_thumbnail_gpu_source("third.png", 8);
        let newest_pair_bytes = second_source.memory_bytes() + third_source.memory_bytes();
        resolver.insert_ready(first.clone(), first_source);
        resolver.insert_ready(second.clone(), second_source);
        resolver.ready_max_bytes = newest_pair_bytes;
        resolver.insert_ready(third.clone(), third_source);

        assert_eq!(resolver.ready_len(), 2);
        assert!(resolver.ready_bytes() <= resolver.ready_max_bytes);
        assert!(!resolver.ready.contains_key(&first));
        assert!(resolver.ready.contains_key(&second));
        assert!(resolver.ready.contains_key(&third));

        // Resolve must keep the ready entry (FileManager-style cache hit) so the
        // next frame does not re-queue I/O or flash a MIME fallback.
        assert!(matches!(
            resolver.resolve(&second.path, 1, Some("image/png".to_string()), 8),
            ThumbnailResolveState::Ready(_)
        ));
        assert_eq!(resolver.ready_len(), 2);
        assert!(resolver.ready_bytes() <= resolver.ready_max_bytes);
        assert!(resolver.ready.contains_key(&second));
        assert!(resolver.ready.contains_key(&third));

        let _ = fs::remove_dir_all(cache_root);
    }

    #[test]
    fn thumbnail_resolver_uses_freedesktop_cache_hit() {
        let cache_root = test_dir("thumbnail-cache-root");
        let source_root = test_dir("thumbnail-source-root");
        fs::create_dir_all(&source_root).unwrap();
        let source = source_root.join("photo.png");
        fs::write(&source, b"source").unwrap();
        let modified_secs = 42;
        let uri = fika_core::thumbnail_uri_for_path(&source).unwrap();
        let thumbnail =
            fika_core::thumbnail_cache_path(&cache_root, fika_core::ThumbnailSize::Normal, &uri);
        write_test_thumbnail_png(&thumbnail, &uri, modified_secs);

        let mut resolver = ThumbnailSourceResolver::with_cache_root(cache_root.clone());
        assert!(matches!(
            resolver.resolve(&source, modified_secs, Some("image/png".to_string()), 48),
            ThumbnailResolveState::Pending
        ));

        match wait_for_thumbnail_state(&mut resolver, &source, modified_secs, Some("image/png"), 48)
        {
            ThumbnailResolveState::Ready(source) => {
                assert_eq!(source.size_px(), 48);
                assert_eq!(source.file_path(), Some(thumbnail.as_path()));
            }
            state => panic!("expected ready thumbnail source, got {state:?}"),
        }

        let _ = fs::remove_dir_all(cache_root);
        let _ = fs::remove_dir_all(source_root);
    }

    #[test]
    fn thumbnail_resolver_caches_failed_probe_result() {
        let cache_root = test_dir("thumbnail-failed-cache-root");
        let source_root = test_dir("thumbnail-failed-source-root");
        fs::create_dir_all(&source_root).unwrap();
        let source = source_root.join("payload.bin");
        fs::write(&source, b"source").unwrap();
        let modified_secs = 42;

        let mut resolver = ThumbnailSourceResolver::with_cache_root(cache_root.clone());
        assert!(matches!(
            resolver.resolve(&source, modified_secs, None, 48),
            ThumbnailResolveState::Pending
        ));
        assert!(matches!(
            wait_for_thumbnail_state(&mut resolver, &source, modified_secs, None, 48),
            ThumbnailResolveState::Failed
        ));
        let pending_after_failure = resolver.pending.len();
        assert!(
            resolver
                .failed
                .contains(&ThumbnailProbeCacheKey::new(source.clone(), modified_secs))
        );
        assert!(matches!(
            resolver.resolve(&source, modified_secs, None, 48),
            ThumbnailResolveState::Failed
        ));
        assert_eq!(resolver.pending.len(), pending_after_failure);

        let _ = fs::remove_dir_all(cache_root);
        let _ = fs::remove_dir_all(source_root);
    }

    fn test_thumbnail_source_request(
        name: &str,
        priority: ThumbnailRequestPriority,
    ) -> ThumbnailSourceRequest {
        ThumbnailSourceRequest {
            key: ThumbnailSourceKey::thumbnail(PathBuf::from(format!("/tmp/{name}")), 48, 1),
            mime_type: Some("image/png".to_string()),
            priority,
        }
    }

    #[derive(Clone, Copy)]
    struct TestGpuIconSpec {
        width: u32,
        height: u32,
        seed: u64,
    }

    fn test_gpu_icon_spec(size: u32, seed: u8) -> TestGpuIconSpec {
        TestGpuIconSpec {
            width: size,
            height: size,
            seed: u64::from(seed),
        }
    }

    fn test_thumbnail_gpu_source(name: &str, size_px: u16) -> IconGpuSource {
        IconGpuSource::file(PathBuf::from("/tmp").join(name), size_px)
    }

    fn test_folder_preview_source(spec: TestGpuIconSpec) -> IconGpuSource {
        IconGpuSource::FolderPreview {
            children: vec![PathBuf::from(format!("/test-preview/{}.png", spec.seed))].into(),
            size_px: spec.width.max(spec.height).min(u32::from(u16::MAX)) as u16,
            seed: spec.seed,
        }
    }
