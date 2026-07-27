
    #[test]
    fn folder_preview_role_rasterizes_chinese_named_jpeg_when_video_is_present() {
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

        assert_eq!(preview.raster.width, 48);
        assert_eq!(preview.raster.height, 48);
        assert!(raster_contains_rgb(&preview.raster, [210, 40, 80]));

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

        assert_eq!(preview.raster.width, 64);
        assert_eq!(preview.raster.height, 64);
        assert!(raster_contains_rgb(&preview.raster, [90, 42, 210]));

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

        assert_eq!(preview.raster.width, 64);
        assert_eq!(preview.raster.height, 64);
        for (_, color) in children {
            assert!(
                raster_contains_rgb(&preview.raster, [color[0], color[1], color[2]]),
                "composed preview should contain child color {color:?}"
            );
        }

        let _ = fs::remove_dir_all(cache_root);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn folder_preview_raster_composes_supplied_child_sources() {
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
        let raster = folder_preview_raster_for_sources(
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

        assert_eq!(raster.width, 48);
        assert_eq!(raster.height, 48);

        let _ = fs::remove_dir_all(cache_root);
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(source_root);
    }

    #[test]
    fn directory_folder_preview_angles_are_stable_and_non_grid_like_dolphin() {
        let seed = folder_preview_directory_seed(Path::new("/home/yk/Documents/wallper"));
        let angles = (0..DOLPHIN_FOLDER_PREVIEW_MAX_IMAGES)
            .map(|index| folder_preview_thumbnail_angle(seed, index))
            .collect::<Vec<_>>();

        assert!(angles.iter().all(|angle| (-8..=8).contains(angle)));
        assert!(angles.iter().any(|angle| *angle != 0));
        assert!(angles.windows(2).any(|pair| pair[0] != pair[1]));
    }

    #[test]
    fn multiple_folder_preview_children_use_dolphin_rotated_segments() {
        let rasters = [
            solid_icon_raster(64, 64, [220, 40, 80, 255]),
            solid_icon_raster(64, 64, [40, 160, 90, 255]),
            solid_icon_raster(64, 64, [50, 100, 220, 255]),
            solid_icon_raster(64, 64, [220, 180, 40, 255]),
        ];
        let seed = folder_preview_directory_seed(Path::new("/home/yk/Documents/wallper"));
        let composed = folder_preview_thumbnail_raster_from_children(&rasters, 128, seed).unwrap();
        let layout = DolphinDirectoryPreviewLayout::new(128).unwrap();
        let slots = folder_preview_thumbnail_slots(rasters.len(), layout);

        assert!(raster_has_visible_pixel_outside_slots(&composed, &slots));
    }

    #[test]
    fn three_folder_preview_children_center_bottom_thumbnail() {
        let layout = DolphinDirectoryPreviewLayout::new(128).unwrap();
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
    fn folder_preview_composition_skips_unpaintable_child_without_dropping_later_images() {
        let transparent = solid_icon_raster(64, 64, [20, 120, 220, 0]);
        let visible = solid_icon_raster(64, 64, [220, 80, 40, 255]);
        let composed =
            folder_preview_thumbnail_raster_from_children(&[transparent, visible], 128, 9).unwrap();
        let layout = DolphinDirectoryPreviewLayout::new(128).unwrap();
        let bottom_row = FolderPreviewThumbnailSlot {
            x: layout.left_margin,
            y: layout.top_margin + layout.segment_height + layout.spacing,
            width: layout
                .folder_size
                .saturating_sub(layout.left_margin + layout.right_margin),
            height: layout.segment_height,
        };

        assert!(raster_contains_rgb(&composed, [220, 80, 40]));
        assert!(raster_contains_rgb_in_rect(
            &composed,
            [220, 80, 40],
            bottom_row
        ));
    }

    #[test]
    fn folder_preview_composition_reflows_three_paintable_children_from_four_candidates() {
        let malformed = IconRaster {
            pixels: vec![20, 120, 220, 255].into(),
            width: 64,
            height: 64,
        };
        let red = solid_icon_raster(64, 64, [220, 40, 80, 255]);
        let green = solid_icon_raster(64, 64, [40, 160, 90, 255]);
        let blue = solid_icon_raster(64, 64, [50, 100, 220, 255]);
        let composed =
            folder_preview_thumbnail_raster_from_children(&[malformed, red, green, blue], 128, 9)
                .unwrap();
        let layout = DolphinDirectoryPreviewLayout::new(128).unwrap();
        let centered_bottom = folder_preview_thumbnail_slots(3, layout)[2];
        let centered_left_half = FolderPreviewThumbnailSlot {
            x: centered_bottom.x,
            y: centered_bottom.y,
            width: centered_bottom.width / 2,
            height: centered_bottom.height,
        };

        assert!(raster_contains_rgb_in_rect(
            &composed,
            [50, 100, 220],
            centered_left_half
        ));
    }

    #[test]
    fn single_opaque_folder_preview_child_uses_dolphin_directory_margins() {
        let raster = solid_icon_raster(64, 64, [210, 40, 80, 255]);
        let framed = folder_preview_thumbnail_raster_from_children(&[raster], 64, 4).unwrap();
        let layout = DolphinDirectoryPreviewLayout::new(64).unwrap();
        let slot = layout.one_tile_slot();

        assert_eq!(framed.width, 64);
        assert_eq!(framed.height, 64);
        assert_eq!(raster_pixel(&framed, 0, 0), [0, 0, 0, 0]);
        assert!(raster_has_visible_pixel_in_rect(&framed, slot));
        assert!(raster_contains_rgb(&framed, [210, 40, 80]));
    }

    #[test]
    fn single_alpha_folder_preview_child_does_not_get_opaque_picture_frame() {
        let raster = solid_icon_raster(64, 64, [20, 120, 220, 128]);
        let framed = folder_preview_thumbnail_raster_from_children(&[raster], 64, 4).unwrap();

        assert_eq!(raster_pixel(&framed, 0, 0), [0, 0, 0, 0]);
        assert!(
            !framed
                .pixels
                .chunks_exact(4)
                .any(|pixel| pixel == [255, 255, 255, 255])
        );
    }

    #[test]
    fn thumbnail_ready_cache_evicts_old_read_ahead_results() {
        let cache_root = test_dir("thumbnail-ready-cache-root");
        let mut resolver = ThumbnailRasterResolver::with_cache_root(cache_root.clone());
        resolver.ready_max_bytes = 32;
        let first = IconRasterCacheKey::thumbnail(PathBuf::from("/tmp/first.png"), 8, 1);
        let second = IconRasterCacheKey::thumbnail(PathBuf::from("/tmp/second.png"), 8, 1);
        let third = IconRasterCacheKey::thumbnail(PathBuf::from("/tmp/third.png"), 8, 1);

        resolver.insert_ready(first.clone(), test_icon_raster(2, 1));
        resolver.insert_ready(second.clone(), test_icon_raster(2, 2));
        resolver.insert_ready(third.clone(), test_icon_raster(2, 3));

        assert_eq!(resolver.ready_len(), 2);
        assert_eq!(resolver.ready_bytes(), 32);
        assert!(!resolver.ready.contains_key(&first));
        assert!(resolver.ready.contains_key(&second));
        assert!(resolver.ready.contains_key(&third));

        // Resolve must keep the ready entry (Dolphin-style cache hit) so the
        // next frame does not re-queue I/O or flash a MIME fallback.
        assert!(matches!(
            resolver.resolve(&second.path, 1, Some("image/png".to_string()), 8),
            ThumbnailResolveState::Ready(_)
        ));
        assert_eq!(resolver.ready_len(), 2);
        assert_eq!(resolver.ready_bytes(), 32);
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

        let mut resolver = ThumbnailRasterResolver::with_cache_root(cache_root.clone());
        assert!(matches!(
            resolver.resolve(&source, modified_secs, Some("image/png".to_string()), 48),
            ThumbnailResolveState::Pending
        ));

        match wait_for_thumbnail_state(&mut resolver, &source, modified_secs, Some("image/png"), 48)
        {
            ThumbnailResolveState::Ready(raster) => {
                assert_eq!(raster.width, 48);
                assert_eq!(raster.height, 48);
                assert!(raster.pixels.iter().any(|channel| *channel != 0));
            }
            state => panic!("expected ready thumbnail raster, got {state:?}"),
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

        let mut resolver = ThumbnailRasterResolver::with_cache_root(cache_root.clone());
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

    fn test_thumbnail_raster_request(
        name: &str,
        priority: ThumbnailRequestPriority,
    ) -> ThumbnailRasterRequest {
        ThumbnailRasterRequest {
            key: IconRasterCacheKey::thumbnail(PathBuf::from(format!("/tmp/{name}")), 48, 1),
            mime_type: Some("image/png".to_string()),
            priority,
        }
    }

    fn test_icon_raster(size: u32, seed: u8) -> IconRaster {
        IconRaster {
            pixels: vec![seed; (size * size * 4) as usize].into(),
            width: size,
            height: size,
        }
    }

    fn solid_icon_raster(width: u32, height: u32, color: [u8; 4]) -> IconRaster {
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for _ in 0..width * height {
            pixels.extend_from_slice(&color);
        }
        IconRaster {
            pixels: pixels.into(),
            width,
            height,
        }
    }

    fn seed_directory_role_raster(cache: &mut IconRoleRasterCache, path: &Path, icon_size: f32) {
        let role = file_icon_path_cache_key(
            path,
            true,
            Some(Arc::from("inode/directory")),
            true,
            icon_size,
        )
        .role;
        cache.begin_frame();
        cache.insert(role, solid_icon_raster(64, 64, [245, 184, 70, 255]));
    }
