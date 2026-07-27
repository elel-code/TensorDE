
    #[test]
    fn icon_raster_cache_reuses_closest_size_for_zoom_transition() {
        let mut cache = IconRasterCache::new(ICON_CACHE_MAX_BYTES);
        let path = PathBuf::from("/theme/mimetypes/text-plain.svg");
        cache.begin_frame();
        cache.insert(
            IconRasterCacheKey::icon(path.clone(), 48),
            test_icon_raster(4, 7),
        );

        assert!(cache.contains_icon_variant(&path));
        let requested = IconRasterCacheKey::icon(path.clone(), 64);
        assert!(cache.get(&requested).is_none());
        let raster = cache
            .get_closest_icon_variant(&requested)
            .expect("zoom should reuse a cached neighboring icon size");

        assert_eq!(raster.width, 4);
        assert_eq!(raster.height, 4);
    }

    #[test]
    fn icon_raster_cache_keeps_rounded_file_icons_separate_from_named_icons() {
        let mut cache = IconRasterCache::new(ICON_CACHE_MAX_BYTES);
        let path = PathBuf::from("/theme/mimetypes/text-plain.svg");
        let original = IconRasterCacheKey::icon(path.clone(), 48);
        let rounded = IconRasterCacheKey::file_icon(
            path.clone(),
            64,
            &FileIconKind::Mime {
                mime: Arc::from("text/plain"),
            },
        );
        let rounded_folder =
            IconRasterCacheKey::file_icon(path, 64, &FileIconKind::Directory);
        cache.begin_frame();
        cache.insert(original, test_icon_raster(4, 7));

        assert_eq!(rounded.style, IconRasterStyle::RoundedFile);
        assert_eq!(rounded_folder.style, IconRasterStyle::RoundedFolder);
        assert!(cache.get_closest_icon_variant(&rounded).is_none());
        assert!(cache.get_closest_icon_variant(&rounded_folder).is_none());
    }

    #[test]
    fn rounded_file_system_icon_raster_masks_only_alpha_content_corners() {
        let width = 20;
        let height = 20;
        let mut pixels = vec![0; (width * height * 4) as usize];
        for y in 2..18 {
            for x in 2..18 {
                let offset = ((y * width + x) * 4) as usize;
                pixels[offset..offset + 4].copy_from_slice(&[40, 120, 220, 255]);
            }
        }
        let raster = IconRaster {
            pixels: Arc::from(pixels),
            width,
            height,
        };

        assert_eq!(
            icon_alpha_content_bounds(raster.pixels.as_ref(), width, height),
            Some((2, 2, 18, 18))
        );
        let rounded =
            rounded_file_system_icon_raster(raster.clone(), FILE_ICON_CORNER_RADIUS_RATIO);
        let alpha_at = |x: u32, y: u32| rounded.pixels[((y * width + x) * 4 + 3) as usize];

        assert!(alpha_at(2, 2) < 64, "outer content corner should be softened");
        assert!(
            alpha_at(3, 2) > alpha_at(2, 2) && alpha_at(3, 2) < 255,
            "neighboring corner pixel should be antialiased"
        );
        assert_eq!(alpha_at(10, 2), 255, "flat top edge should stay crisp");
        assert_eq!(alpha_at(10, 10), 255, "icon interior should be unchanged");
        assert_eq!(alpha_at(1, 1), 0, "transparent padding should stay clear");
        assert_eq!(raster.pixels[((2 * width + 2) * 4 + 3) as usize], 255);
    }

    #[test]
    fn icon_role_raster_cache_reuses_previous_role_for_zoom_transition() {
        let mut cache = IconRoleRasterCache::new(ICON_ROLE_RASTER_CACHE_MAX_BYTES);
        let role = file_icon_path_cache_key(
            Path::new("/tmp/readme.txt"),
            false,
            Some(Arc::from("text/plain")),
            true,
            48.0,
        )
        .role;

        cache.begin_frame();
        cache.insert(role.clone(), test_icon_raster(4, 7));
        cache.begin_frame();
        let raster = cache
            .get(&role)
            .expect("zoom should reuse the previous raster for the same MIME role");

        assert_eq!(raster.width, 4);
        assert_eq!(raster.height, 4);
    }

    #[test]
    fn dialog_rects_scale_with_window_dpi() {
        let chooser = ShellOpenWithChooser::new(
            PathBuf::from("/tmp/plain.txt"),
            Some(Arc::from("text/plain")),
            vec![MimeApplication {
                id: "org.example.Editor.desktop".to_string(),
                desktop_file: PathBuf::from("/usr/share/applications/org.example.Editor.desktop"),
                name: "Editor".to_string(),
                exec: "editor %F".to_string(),
                icon: None,
                is_default: false,
            }],
            Vec::new(),
        );
        let base_size = open_with_chooser_window_size_scaled(&chooser, 1.0);
        let scaled_size = open_with_chooser_window_size_scaled(&chooser, 1.5);
        let base = open_with_chooser_rect(&chooser, base_size);
        let scaled = open_with_chooser_rect_scaled(&chooser, scaled_size, 1.5);
        assert_eq!(
            base.width,
            scaled_dialog_metric(OPEN_WITH_CHOOSER_WIDTH, 1.0)
        );
        assert_eq!(
            scaled.width,
            scaled_dialog_metric(OPEN_WITH_CHOOSER_WIDTH, 1.5)
        );
        assert!(scaled.width > base.width);
        assert!(scaled.height > base.height);
        assert_eq!(base.x, 0.0);
        assert_eq!(base.y, 0.0);
        assert_eq!(scaled.x, 0.0);
        assert_eq!(scaled.y, 0.0);
        assert_eq!(
            open_with_chooser_list_rect_scaled(scaled, &chooser, 1.5).height,
            scaled_dialog_metric(OPEN_WITH_CHOOSER_ROW_HEIGHT, 1.5)
                * OPEN_WITH_CHOOSER_MAX_ROWS as f32
        );
    }

    #[test]
    fn open_with_dialog_size_is_stable_when_search_results_change() {
        let applications = (0..12)
            .map(|index| MimeApplication {
                id: format!("app{index}.desktop"),
                desktop_file: PathBuf::from(format!("/apps/app{index}.desktop")),
                name: if index == 3 {
                    "Unique Paint".to_string()
                } else {
                    format!("Editor {index}")
                },
                exec: format!("app{index} %f"),
                icon: None,
                is_default: false,
            })
            .collect::<Vec<_>>();
        let mut chooser = ShellOpenWithChooser::new(
            PathBuf::from("/tmp/plain.txt"),
            Some(Arc::from("text/plain")),
            applications,
            Vec::new(),
        );
        let full_size = open_with_chooser_window_size_scaled(&chooser, 1.0);
        let full_list = open_with_chooser_list_rect_scaled(
            open_with_chooser_rect(&chooser, full_size),
            &chooser,
            1.0,
        );
        assert!(chooser.apply_command(OpenWithCommand::Insert("Unique".to_string())));
        assert_eq!(chooser.filtered_count(), 1);
        assert_eq!(
            open_with_chooser_window_size_scaled(&chooser, 1.0),
            full_size
        );
        assert_eq!(
            open_with_chooser_list_rect_scaled(
                open_with_chooser_rect(&chooser, full_size),
                &chooser,
                1.0,
            ),
            full_list
        );
        chooser.error = Some("Launch failed".to_string());
        assert_eq!(
            open_with_chooser_window_size_scaled(&chooser, 1.0),
            full_size
        );
    }

    #[test]
    fn file_icon_path_cache_keys_share_dolphin_role_across_sizes() {
        let path = Path::new("/tmp/plain.txt");
        let small =
            file_icon_path_cache_key(path, false, Some(Arc::from("text/plain")), true, 18.0);
        let large =
            file_icon_path_cache_key(path, false, Some(Arc::from("text/plain")), true, 48.0);

        assert_eq!(small.role, large.role);
        assert_ne!(small.size_px, large.size_px);
    }

    #[test]
    fn dolphin_zoom_level_sizes_match_dolphin_table() {
        assert_eq!(dolphin_icon_size_for_zoom_level(0), 16.0);
        assert_eq!(dolphin_icon_size_for_zoom_level(1), 22.0);
        assert_eq!(dolphin_icon_size_for_zoom_level(2), 32.0);
        assert_eq!(dolphin_icon_size_for_zoom_level(3), 48.0);
        assert_eq!(dolphin_icon_size_for_zoom_level(4), 64.0);
        assert_eq!(dolphin_icon_size_for_zoom_level(5), 80.0);
        assert_eq!(dolphin_icon_size_for_zoom_level(16), 256.0);
    }

    #[test]
    fn icon_cache_size_quantizes_to_dolphin_zoom_sizes() {
        assert_eq!(icon_cache_size(18.0), 16);
        assert_eq!(icon_cache_size(28.0), 32);
        assert_eq!(icon_cache_size(48.0), 48);
        assert_eq!(icon_cache_size(64.0), 64);
        assert_eq!(icon_cache_size(250.0), 256);
    }

    #[test]
    fn default_icon_theme_fallbacks_prefer_deepin_before_breeze() {
        let mut themes = Vec::new();
        push_default_icon_theme_fallbacks(&mut themes);

        assert_eq!(themes[0], "bloom");
        assert_eq!(themes[1], "bloom-dark");
        assert_eq!(themes[2], "deepin");
        assert_eq!(themes[3], "deepin-dark");
        assert!(
            themes.iter().position(|theme| theme == "deepin").unwrap()
                < themes.iter().position(|theme| theme == "breeze").unwrap()
        );
    }

    #[test]
    fn file_icon_path_cache_keys_share_dolphin_mime_role_across_paths_and_extensions() {
        let text_file = file_icon_path_cache_key(
            Path::new("/tmp/readme.txt"),
            false,
            Some(Arc::from("text/plain")),
            true,
            32.0,
        );
        let log_file = file_icon_path_cache_key(
            Path::new("/var/log/system.log"),
            false,
            Some(Arc::from("text/plain")),
            true,
            32.0,
        );
        let image_file = file_icon_path_cache_key(
            Path::new("/tmp/readme.png"),
            false,
            Some(Arc::from("image/png")),
            true,
            32.0,
        );

        assert_eq!(text_file.role, log_file.role);
        assert_ne!(text_file.role, image_file.role);
    }

    #[test]
    fn preliminary_file_icon_roles_keep_extension_until_mime_is_resolved() {
        let text_file = file_icon_path_cache_key(
            Path::new("/tmp/readme.txt"),
            false,
            Some(Arc::from(fika_core::GENERIC_BINARY_MIME)),
            false,
            32.0,
        );
        let log_file = file_icon_path_cache_key(
            Path::new("/var/log/system.log"),
            false,
            Some(Arc::from(fika_core::GENERIC_BINARY_MIME)),
            false,
            32.0,
        );

        assert_ne!(text_file.role, log_file.role);
    }

    #[test]
    fn generic_binary_file_icon_roles_share_dolphin_generic_role() {
        let script_file = file_icon_path_cache_key(
            Path::new("/usr/bin/tool.py"),
            false,
            Some(Arc::from(fika_core::GENERIC_BINARY_MIME)),
            true,
            32.0,
        );
        let shell_file = file_icon_path_cache_key(
            Path::new("/usr/bin/tool.sh"),
            false,
            Some(Arc::from(fika_core::GENERIC_BINARY_MIME)),
            true,
            32.0,
        );
        let unknown_file =
            file_icon_path_cache_key(Path::new("/usr/bin/tool.pl"), false, None, true, 32.0);

        assert_eq!(script_file.role, shell_file.role);
        assert_eq!(script_file.role, unknown_file.role);
        assert!(matches!(
            script_file.role.kind,
            FileIconKind::File { extension: None }
        ));
    }

    #[test]
    fn file_icon_resolver_reuses_mime_pending_and_cached_snapshot() {
        let mut harness = FileIconResolverTestHarness::new();
        let text_file = test_entry_with_mime("readme.txt", false, "text/plain");
        let log_file = test_entry_with_mime("system.log", false, "text/plain");
        let icon_size = 32.0;

        assert_eq!(
            harness
                .resolver
                .resolve_entry(Path::new("/tmp"), &text_file, icon_size),
            None
        );
        let request_key = harness
            .next_request_key()
            .expect("first visible text file should queue one icon resolve");
        match &request_key.role.kind {
            FileIconKind::Mime { mime } => assert_eq!(mime.as_ref(), "text/plain"),
            kind => panic!("expected text/plain MIME role, got {kind:?}"),
        }
        assert_eq!(harness.resolver.pending_len_for_test(), 1);

        assert_eq!(
            harness
                .resolver
                .resolve_entry(Path::new("/var/log"), &log_file, icon_size),
            None
        );
        assert_eq!(harness.resolver.pending_len_for_test(), 1);
        assert!(
            harness.next_request_key().is_none(),
            "same MIME role and size should reuse the pending request"
        );

        let resolved_path = PathBuf::from("/theme/mimetypes/text-plain.svg");
        harness.complete(request_key, Some(resolved_path.clone()));
        let resolved = harness
            .resolver
            .resolve_entry(Path::new("/var/log"), &log_file, icon_size)
            .expect("same MIME role should reuse the cached resolved icon");

        assert_eq!(resolved.path, Some(resolved_path));
        assert_eq!(harness.resolver.pending_len_for_test(), 0);
        assert_eq!(harness.resolver.cached_len_for_test(), 1);
        assert!(harness.next_request_key().is_none());
    }

    #[test]
    fn file_icon_resolver_fast_path_caches_mime_role_without_pending_jump() {
        let mut harness = FileIconResolverTestHarness::new();
        let text_file = test_entry_with_mime("readme.txt", false, "text/plain");
        let log_file = test_entry_with_mime("system.log", false, "text/plain");
        let icon_size = 32.0;

        let resolved =
            harness
                .resolver
                .resolve_entry_fast(Path::new("/tmp"), &text_file, icon_size);

        assert_eq!(harness.resolver.pending_len_for_test(), 0);
        assert_eq!(harness.resolver.cached_len_for_test(), 1);
        assert!(
            harness.next_request_key().is_none(),
            "visible fast path should not enqueue async icon resolution"
        );
        assert_eq!(
            harness
                .resolver
                .resolve_entry(Path::new("/var/log"), &log_file, icon_size),
            Some(resolved)
        );
        assert!(harness.next_request_key().is_none());
    }

    #[test]
    fn file_icon_resolver_visible_fast_path_resolves_exact_role_without_pending_jump() {
        let mut harness = FileIconResolverTestHarness::new();
        let text_file = test_entry_with_mime("readme.txt", false, "text/plain");
        let log_file = test_entry_with_mime("system.log", false, "text/plain");
        let icon_size = 32.0;

        let resolved =
            harness
                .resolver
                .resolve_entry_visible_fast(Path::new("/tmp"), &text_file, icon_size);

        assert_eq!(harness.resolver.pending_len_for_test(), 0);
        assert_eq!(harness.resolver.cached_len_for_test(), 1);
        assert!(
            harness.next_request_key().is_none(),
            "visible fast prewarm should not enqueue async icon resolution"
        );

        let (visible, deferred) =
            harness
                .resolver
                .resolve_entry_visible(Path::new("/var/log"), &log_file, icon_size);

        assert!(!deferred);
        assert_eq!(visible, resolved);
        assert_eq!(harness.resolver.pending_len_for_test(), 0);
        assert!(harness.next_request_key().is_none());
    }

    #[test]
    fn file_icon_resolver_visible_path_sync_resolves_exact_mime_without_fallback() {
        let mut harness = FileIconResolverTestHarness::new();
        let shortcut =
            test_entry_with_mime("OCS Desktop.lnk", false, "application/x-ms-shortcut");
        let icon_size = 32.0;

        let (_resolved, deferred) =
            harness
                .resolver
                .resolve_entry_visible(Path::new("/tmp"), &shortcut, icon_size);
        assert!(!deferred);
        assert_eq!(harness.resolver.pending_len_for_test(), 0);
        assert_eq!(harness.resolver.cached_len_for_test(), 1);
        assert!(
            harness.next_request_key().is_none(),
            "Dolphin resolves visible MIME icons synchronously before async previews"
        );
    }

    #[test]
    fn open_in_new_pane_loads_reusable_pane_state() {
        let root = test_dir("split-pane");
        let right = root.join("right");
        fs::create_dir_all(&right).unwrap();
        fs::write(right.join("child.txt"), "split").unwrap();

        let mut scene = test_scene(vec![test_entry("right", true)], ShellViewMode::Icons);
        scene.panes[ShellPaneId::SLOT_0].path = root.clone();
        scene.context_target = Some(ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: right.clone(),
            is_dir: true,
            selection_count: 1,
        });
        let size = PhysicalSize::new(900, 420);

        assert!(scene.open_split_pane_from_context(size).unwrap());
        let pane = scene
            .panes
            .get(ShellPaneId::SLOT_1)
            .expect("split pane should load");
        assert_eq!(pane.path, right);
        assert_eq!(pane.view_mode, ShellViewMode::Icons);
        assert_eq!(pane.entries.len(), 1);
        assert_eq!(pane.entries[0].name.as_ref(), "child.txt");
        assert_eq!(pane.filtered_entry_count(), 1);
        assert_eq!(scene.split_pane_changes, 1);
        let metrics = scene
            .split_pane_metrics(size)
            .expect("split pane should expose geometry");
        assert!(scene.pane_width(size) < (size.width as f32 - scene.content_origin_x(size)));
        assert!(metrics.right_pane.x > scene.pane_rect(size).x);

        {
            let pane = scene
                .panes
                .get_mut(ShellPaneId::SLOT_1)
                .expect("split pane should load");
            pane.view_mode = ShellViewMode::Compact;
            pane.scroll_x = 12.0;
        }
        let pane = scene
            .panes
            .get(ShellPaneId::SLOT_1)
            .expect("split pane should load");
        let view = ShellPaneView::from_state(pane);
        assert_eq!(view.path, right.as_path());
        assert_eq!(view.dir_count, 0);
        assert_eq!(view.scroll_x, 12.0);
        let layout = scene.pane_layout_for_pane(
            ShellPaneId::SLOT_1,
            view,
            metrics.right_pane.width,
            metrics.right_pane.height,
        );
        match layout {
            ShellLayout::Compact(layout) => {
                assert_eq!(layout.visible_items().len(), 1);
                assert!(layout.content_size().width > 0.0);
            }
            _ => panic!("split pane view mode should drive reusable compact layout"),
        }

        fs::remove_dir_all(root).unwrap();
    }
