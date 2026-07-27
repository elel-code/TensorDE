
    #[test]
    fn blank_context_menu_offers_directory_open_with_root_applications() {
        let target = ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_0,
            path: PathBuf::from("/tmp/project"),
        };
        let app = |id: &str, name: &str, icon: Option<&str>| MimeApplication {
            id: format!("org.example.{id}.desktop"),
            desktop_file: PathBuf::from(format!(
                "/usr/share/applications/org.example.{id}.desktop"
            )),
            name: name.to_string(),
            exec: format!("{} %F", name.to_ascii_lowercase()),
            icon: icon.map(str::to_string),
            is_default: false,
        };
        let menu = ShellContextMenu::with_dynamic(
            target,
            ViewPoint { x: 20.0, y: 20.0 },
            vec![
                app("Code", "Code", Some("com.visualstudio.code")),
                app("Kate", "Kate", Some("kate")),
            ],
            Vec::new(),
        );

        let root = context_menu_items(&menu);
        assert!(root.iter().any(|item| {
            matches!(
                item.command,
                ShellContextMenuCommand::OpenWithApplication { .. }
            ) && item.label == "Open With Code"
        }));
        assert!(
            root.iter()
                .any(|item| item.submenu == Some(ShellContextSubmenu::OpenWith))
        );
    }

    #[test]
    fn context_menu_items_offer_service_root_more_and_group_submenus() {
        let target = ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: PathBuf::from("/tmp/archive.zip"),
            is_dir: false,
            selection_count: 1,
        };
        let mut service_actions = Vec::new();
        service_actions.push(ServiceMenuAction {
            id: "compress.desktop::compress".to_string(),
            label: "Compress".to_string(),
            source_name: "Ark".to_string(),
            icon: Some("ark".to_string()),
            submenu: None,
            priority: ServiceMenuPriority::Normal,
        });
        service_actions.push(ServiceMenuAction {
            id: "tools.desktop::checksum".to_string(),
            label: "Checksum".to_string(),
            source_name: "Tools".to_string(),
            icon: None,
            submenu: Some("Tools".to_string()),
            priority: ServiceMenuPriority::Normal,
        });
        for index in 0..4 {
            service_actions.push(ServiceMenuAction {
                id: format!("extra.desktop::action{index}"),
                label: format!("Extra {index}"),
                source_name: "Extra".to_string(),
                icon: None,
                submenu: None,
                priority: ServiceMenuPriority::Normal,
            });
        }
        let menu = ShellContextMenu::with_dynamic(
            target,
            ViewPoint { x: 20.0, y: 20.0 },
            Vec::new(),
            service_actions,
        );

        let root = context_menu_items(&menu);
        assert!(root.iter().any(|item| matches!(
            item.command,
            ShellContextMenuCommand::RunServiceMenuAction { .. }
        )));
        assert!(root.iter().any(|item| {
            item.submenu == Some(ShellContextSubmenu::ServiceMenu) && item.label == "More Actions"
        }));
        let more = context_submenu_actions(ShellContextSubmenu::ServiceMenu, &menu);
        assert!(more.iter().any(|item| {
            item.submenu == Some(ShellContextSubmenu::ServiceMenuGroup(0)) && item.label == "Tools"
        }));
        let tools = context_submenu_actions(ShellContextSubmenu::ServiceMenuGroup(0), &menu);
        assert!(tools.iter().any(|item| item.label == "Checksum"));
    }

    #[test]
    fn service_menu_named_icon_request_preserves_icon_name() {
        let action = ServiceMenuAction {
            id: "archive.desktop::compress".to_string(),
            label: "Compress".to_string(),
            source_name: "Archive".to_string(),
            icon: Some("archive-insert".to_string()),
            submenu: None,
            priority: ServiceMenuPriority::TopLevel,
        };
        let item = service_menu_action_item(&action);

        assert_eq!(
            context_menu_named_icon_request(&item),
            Some(("archive-insert", NamedIconFallback::Service))
        );
    }

    #[test]
    fn service_menu_named_icon_request_supplies_service_fallback_icon() {
        let item = ShellContextMenuItem {
            command: ShellContextMenuCommand::OpenSubmenu(ShellContextSubmenu::ServiceMenu),
            label: "More Actions".to_string(),
            separator_before: false,
            submenu: Some(ShellContextSubmenu::ServiceMenu),
            icon: ShellContextMenuIcon::Service(None),
        };

        assert_eq!(
            context_menu_named_icon_request(&item),
            Some(("system-run", NamedIconFallback::Service))
        );
    }

    #[test]
    fn named_service_icon_candidates_prefer_service_icon() {
        let profile = file_icon_profile(
            &FileIconKind::Named {
                icon_name: "tools-checksum".to_string(),
                fallback: NamedIconFallback::Service,
            },
            fika_core::MimeDatabase::shared(),
        );

        assert_eq!(
            profile.icon_candidates.first().map(String::as_str),
            Some("tools-checksum")
        );
        assert!(
            profile
                .generic_candidates
                .iter()
                .any(|name| name == "configure")
        );
        assert!(
            profile
                .generic_candidates
                .iter()
                .any(|name| name == "system-run")
        );
    }

    #[test]
    fn icon_frame_vertices_sample_raw_texture_for_gpu_clamp() {
        let mut resolver = FileIconResolver::new();
        let mut thumbnails = ThumbnailRasterResolver::new();
        let mut icon_rasters = IconRasterResolver::new();
        let mut raster_cache = IconRasterCache::new(ICON_CACHE_MAX_BYTES);
        let mut role_raster_cache = IconRoleRasterCache::new(ICON_ROLE_RASTER_CACHE_MAX_BYTES);
        let mut builder = IconFrameBuilder::new_for_test(
            &mut resolver,
            &mut thumbnails,
            &mut icon_rasters,
            &mut raster_cache,
            &mut role_raster_cache,
            PhysicalSize::new(128, 96),
        );
        let raster = test_icon_raster(2, 7);
        let identity = IconGpuUploadKey::theme_asset(PathBuf::from("/test/icon.png"));
        builder.copy_raster_to_atlas(
            identity,
            raster,
            ViewRect {
                x: 4.0,
                y: 4.0,
                width: 16.0,
                height: 16.0,
            },
            ViewRect {
                x: 4.0,
                y: 4.0,
                width: 16.0,
                height: 16.0,
            },
            IconDrawLayer::Content,
        );

        let frame = builder.finish();
        assert_eq!(frame.slots.len(), 1);
        let slot = &frame.slots[0];
        let tex_w = slot.width as f32;
        let tex_h = slot.height as f32;
        let u0 = frame.content_vertices[0].uv[0] * tex_w;
        let v0 = frame.content_vertices[0].uv[1] * tex_h;
        let u1 = frame.content_vertices[2].uv[0] * tex_w;
        let v1 = frame.content_vertices[2].uv[1] * tex_h;

        // Scheme-C uses one texture per icon. ClampToEdge performs edge
        // extension in the sampler, avoiding a CPU padding allocation/copy.
        assert_eq!(slot.width, 2);
        assert_eq!(slot.height, 2);
        assert!(slot.upload.is_some(), "cold slot carries one-shot CPU upload");
        assert!(u0.abs() < 0.001);
        assert!(v0.abs() < 0.001);
        assert!((u1 - 2.0).abs() < 0.001);
        assert!((v1 - 2.0).abs() < 0.001);
    }

    #[test]
    fn icon_frame_keeps_overlay_vertices_separate() {
        let mut resolver = FileIconResolver::new();
        let mut thumbnails = ThumbnailRasterResolver::new();
        let mut icon_rasters = IconRasterResolver::new();
        let mut raster_cache = IconRasterCache::new(ICON_CACHE_MAX_BYTES);
        let mut role_raster_cache = IconRoleRasterCache::new(ICON_ROLE_RASTER_CACHE_MAX_BYTES);
        let mut builder = IconFrameBuilder::new_for_test(
            &mut resolver,
            &mut thumbnails,
            &mut icon_rasters,
            &mut raster_cache,
            &mut role_raster_cache,
            PhysicalSize::new(128, 96),
        );
        let raster = test_icon_raster(2, 7);
        let identity = IconGpuUploadKey::theme_asset(PathBuf::from("/test/icon.png"));
        builder.copy_raster_to_atlas(
            identity.clone(),
            raster.clone(),
            ViewRect {
                x: 4.0,
                y: 4.0,
                width: 16.0,
                height: 16.0,
            },
            ViewRect {
                x: 4.0,
                y: 4.0,
                width: 16.0,
                height: 16.0,
            },
            IconDrawLayer::Content,
        );
        builder.copy_raster_to_atlas(
            identity,
            raster,
            ViewRect {
                x: 24.0,
                y: 4.0,
                width: 16.0,
                height: 16.0,
            },
            ViewRect {
                x: 24.0,
                y: 4.0,
                width: 16.0,
                height: 16.0,
            },
            IconDrawLayer::Overlay,
        );

        let frame = builder.finish();

        assert_eq!(frame.content_vertices.len(), 6);
        assert_eq!(frame.overlay_vertices.len(), 6);
        // Same raster → one GPU slot shared by content + overlay.
        assert_eq!(frame.slots.len(), 1);
        assert_eq!(frame.content_batches.len(), 1);
        assert_eq!(frame.overlay_batches.len(), 1);
        assert_eq!(frame.stats.quads, 2);
    }

    #[test]
    fn ready_folder_preview_keeps_directory_icon_shell() {
        let mut resolver = FileIconResolver::new();
        let mut thumbnails = ThumbnailRasterResolver::new();
        let mut icon_rasters = IconRasterResolver::new();
        let mut raster_cache = IconRasterCache::new(ICON_CACHE_MAX_BYTES);
        let mut role_raster_cache = IconRoleRasterCache::new(ICON_ROLE_RASTER_CACHE_MAX_BYTES);
        seed_directory_role_raster(&mut role_raster_cache, Path::new("/tmp/album"), 96.0);
        let mut builder = IconFrameBuilder::new_for_test(
            &mut resolver,
            &mut thumbnails,
            &mut icon_rasters,
            &mut raster_cache,
            &mut role_raster_cache,
            PhysicalSize::new(240, 180),
        );
        let entry = test_entry_with_mime_and_modified("album", true, "inode/directory", Some(7));
        let preview = FolderPreviewReady {
            stamp: 11,
            size_px: 96,
            raster: IconRaster {
                pixels: vec![31; 96 * 48 * 4].into(),
                width: 96,
                height: 48,
            },
        };
        let layout = ItemPixmapLayout {
            view_mode: ShellViewMode::Icons,
            icon_rect: ViewRect {
                x: 44.0,
                y: 10.0,
                width: 96.0,
                height: 96.0,
            },
            text_rect: ViewRect {
                x: 14.0,
                y: 108.0,
                width: 156.0,
                height: 18.0,
            },
            text_midline_shift: 0.0,
        };

        assert!(builder.push_thumbnail_or_icon(
            Path::new("/tmp"),
            &entry,
            Some(&preview),
            layout,
            ViewRect {
                x: 0.0,
                y: 0.0,
                width: 240.0,
                height: 180.0,
            },
        ));
        let frame = builder.finish();

        assert_eq!(frame.stats.icons, 1);
        assert_eq!(frame.stats.folder_preview_quads, 1);
        assert_eq!(frame.stats.quads, 2);
    }

    #[test]
    fn compact_ready_folder_preview_keeps_directory_icon_below_32px() {
        let mut resolver = FileIconResolver::new();
        let mut thumbnails = ThumbnailRasterResolver::new();
        let mut icon_rasters = IconRasterResolver::new();
        let mut raster_cache = IconRasterCache::new(ICON_CACHE_MAX_BYTES);
        let mut role_raster_cache = IconRoleRasterCache::new(ICON_ROLE_RASTER_CACHE_MAX_BYTES);
        seed_directory_role_raster(&mut role_raster_cache, Path::new("/tmp/album"), 28.0);
        let mut builder = IconFrameBuilder::new_for_test(
            &mut resolver,
            &mut thumbnails,
            &mut icon_rasters,
            &mut raster_cache,
            &mut role_raster_cache,
            PhysicalSize::new(160, 80),
        );
        let entry = test_entry_with_mime_and_modified("album", true, "inode/directory", Some(7));
        let preview = FolderPreviewReady {
            stamp: 11,
            size_px: 128,
            raster: test_icon_raster(28, 20),
        };
        let layout = ItemPixmapLayout {
            view_mode: ShellViewMode::Compact,
            icon_rect: ViewRect {
                x: 6.0,
                y: 6.0,
                width: 28.0,
                height: 28.0,
            },
            text_rect: ViewRect {
                x: 42.0,
                y: 9.0,
                width: 88.0,
                height: 18.0,
            },
            text_midline_shift: 0.0,
        };

        assert!(builder.push_thumbnail_or_icon(
            Path::new("/tmp"),
            &entry,
            Some(&preview),
            layout,
            ViewRect {
                x: 0.0,
                y: 0.0,
                width: 160.0,
                height: 80.0,
            },
        ));
        let frame = builder.finish();

        assert_eq!(frame.stats.icons, 1);
        assert_eq!(frame.stats.folder_preview_quads, 1);
        assert_eq!(frame.stats.quads, 2);
    }

    #[test]
    fn named_overlay_icon_queues_raster_when_sync_budget_is_empty() {
        let mut harness = FileIconResolverTestHarness::new();
        let mut thumbnails = ThumbnailRasterResolver::new();
        let mut icon_rasters = IconRasterResolver::new();
        let mut raster_cache = IconRasterCache::new(ICON_CACHE_MAX_BYTES);
        let mut role_raster_cache = IconRoleRasterCache::new(ICON_ROLE_RASTER_CACHE_MAX_BYTES);
        let icon = ViewRect {
            x: 4.0,
            y: 4.0,
            width: 16.0,
            height: 16.0,
        };
        let clip = ViewRect {
            x: 0.0,
            y: 0.0,
            width: 128.0,
            height: 96.0,
        };

        {
            let mut builder = IconFrameBuilder::new_for_test(
                &mut harness.resolver,
                &mut thumbnails,
                &mut icon_rasters,
                &mut raster_cache,
                &mut role_raster_cache,
                PhysicalSize::new(128, 96),
            );
            assert!(!builder.push_named_theme_icon(
                "archive-insert",
                NamedIconFallback::Service,
                icon,
                clip,
                IconDrawLayer::Overlay,
            ));
        }
        let request_key = harness
            .next_request_key()
            .expect("named overlay icon should queue a theme resolve");
        let resolved_path = PathBuf::from("/theme/actions/archive-insert.svg");
        harness.complete(request_key, Some(resolved_path.clone()));

        {
            let mut builder = IconFrameBuilder::new_for_test(
                &mut harness.resolver,
                &mut thumbnails,
                &mut icon_rasters,
                &mut raster_cache,
                &mut role_raster_cache,
                PhysicalSize::new(128, 96),
            );
            assert!(!builder.push_named_theme_icon(
                "archive-insert",
                NamedIconFallback::Service,
                icon,
                clip,
                IconDrawLayer::Overlay,
            ));
        }

        assert!(
            icon_rasters
                .pending
                .contains_key(&IconRasterCacheKey::icon(resolved_path, 16))
        );
    }

    #[test]
    fn synchronous_file_icon_raster_does_not_queue_duplicate_worker_request() {
        let mut harness = FileIconResolverTestHarness::new();
        let directory = PathBuf::from("/tmp/fika-sync-icon-raster");
        let entry = test_entry_with_mime("payload", false, "text/plain");
        assert!(
            harness
                .resolver
                .resolve_entry(&directory, &entry, 16.0)
                .is_none()
        );
        let request_key = harness.next_request_key().unwrap();
        harness.complete(
            request_key,
            Some(PathBuf::from("/missing/fika-sync-icon-raster.svg")),
        );
        harness.resolver.drain_results();

        let mut thumbnails = ThumbnailRasterResolver::new();
        let mut icon_rasters = IconRasterResolver::new();
        let mut raster_cache = IconRasterCache::new(ICON_CACHE_MAX_BYTES);
        let mut role_raster_cache = IconRoleRasterCache::new(ICON_ROLE_RASTER_CACHE_MAX_BYTES);
        let rect = ViewRect {
            x: 4.0,
            y: 4.0,
            width: 16.0,
            height: 16.0,
        };
        {
            let mut builder = IconFrameBuilder::new(
                IconFrameResources::new(
                    &mut harness.resolver,
                    &mut thumbnails,
                    &mut icon_rasters,
                    &mut raster_cache,
                    &mut role_raster_cache,
                    IconGpuResidentIndex::default(),
                ),
                IconFrameConfig::new(PhysicalSize::new(128, 96), 1.0, 1),
            );
            assert!(!builder.push_icon(
                &directory,
                &entry,
                rect,
                ViewRect {
                    x: 0.0,
                    y: 0.0,
                    width: 128.0,
                    height: 96.0,
                },
                IconDrawLayer::Content,
            ));
        }

        assert!(icon_rasters.pending.is_empty());
    }

    #[test]
    fn gpu_resident_thumbnail_does_not_requeue_same_size_cpu_work() {
        let directory = PathBuf::from("/tmp");
        let path = directory.join("resident.png");
        let modified_secs = 17;
        let gpu_key = IconGpuUploadKey::content(path.clone(), modified_secs);
        let resident = IconGpuResidentIndex {
            entries: HashMap::from([(
                gpu_key,
                IconGpuResidentEntry {
                    width: 256,
                    height: 256,
                    content_width: 256,
                    content_height: 256,
                    content_hash: 1,
                },
            )]),
        };
        let mut resolver = FileIconResolver::new();
        let mut thumbnails = ThumbnailRasterResolver::new();
        let mut icon_rasters = IconRasterResolver::new();
        let mut raster_cache = IconRasterCache::new(ICON_CACHE_MAX_BYTES);
        let mut role_raster_cache = IconRoleRasterCache::new(ICON_ROLE_RASTER_CACHE_MAX_BYTES);
        let entry = test_entry_with_mime_and_modified(
            "resident.png",
            false,
            "image/png",
            Some(modified_secs),
        );
        let mut builder = IconFrameBuilder::new(
            IconFrameResources::new(
                &mut resolver,
                &mut thumbnails,
                &mut icon_rasters,
                &mut raster_cache,
                &mut role_raster_cache,
                resident,
            ),
            IconFrameConfig::new(PhysicalSize::new(128, 96), 1.0, 0),
        );

        assert!(builder.push_thumbnail(
            &directory,
            &entry,
            ViewRect {
                x: 4.0,
                y: 4.0,
                width: 48.0,
                height: 48.0,
            },
            ViewRect {
                x: 0.0,
                y: 0.0,
                width: 128.0,
                height: 96.0,
            },
            IconDrawLayer::Content,
        ));
        assert!(thumbnails.pending.is_empty());
    }
