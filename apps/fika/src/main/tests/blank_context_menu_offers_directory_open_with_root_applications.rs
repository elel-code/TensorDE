#[test]
fn blank_context_menu_offers_directory_open_with_root_applications() {
    let target = ShellContextTarget::Blank {
        pane: ShellPaneId::SLOT_0,
        path: PathBuf::from("/tmp/project"),
    };
    let app = |id: &str, name: &str, icon: Option<&str>| MimeApplication {
        id: format!("org.example.{id}.desktop"),
        desktop_file: PathBuf::from(format!("/usr/share/applications/org.example.{id}.desktop")),
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
fn icon_frame_vertices_sample_gpu_source_texture_for_clamp() {
    let mut resolver = FileIconResolver::new();
    let mut thumbnails = ThumbnailSourceResolver::new();
    let mut builder =
        IconFrameBuilder::new_for_test(&mut resolver, &mut thumbnails, PhysicalSize::new(128, 96));
    let identity = IconGpuUploadKey::theme_asset(PathBuf::from("/test/icon.png"), 2);
    let rect = ViewRect {
        x: 4.0,
        y: 4.0,
        width: 16.0,
        height: 16.0,
    };
    builder.push_gpu_source_draw(
        identity,
        IconGpuSource::file(PathBuf::from("/test/icon.png"), 2),
        rect,
        rect,
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

    assert_eq!((slot.width, slot.height), (2, 2));
    assert!(slot.source.is_some());
    assert!(u0.abs() < 0.001);
    assert!(v0.abs() < 0.001);
    assert!((u1 - 2.0).abs() < 0.001);
    assert!((v1 - 2.0).abs() < 0.001);
}

#[test]
fn icon_frame_keeps_gpu_overlay_vertices_separate() {
    let mut resolver = FileIconResolver::new();
    let mut thumbnails = ThumbnailSourceResolver::new();
    let mut builder =
        IconFrameBuilder::new_for_test(&mut resolver, &mut thumbnails, PhysicalSize::new(128, 96));
    let identity = IconGpuUploadKey::theme_asset(PathBuf::from("/test/icon.png"), 2);
    let source = IconGpuSource::file(PathBuf::from("/test/icon.png"), 2);
    let content = ViewRect {
        x: 4.0,
        y: 4.0,
        width: 16.0,
        height: 16.0,
    };
    let overlay = ViewRect {
        x: 24.0,
        y: 4.0,
        width: 16.0,
        height: 16.0,
    };
    builder.push_gpu_source_draw(
        identity.clone(),
        source.clone(),
        content,
        content,
        IconDrawLayer::Content,
    );
    builder.push_gpu_source_draw(identity, source, overlay, overlay, IconDrawLayer::Overlay);

    let frame = builder.finish();
    assert_eq!(frame.content_vertices.len(), 6);
    assert_eq!(frame.overlay_vertices.len(), 6);
    assert_eq!(frame.slots.len(), 1);
    assert_eq!(frame.content_batches.len(), 1);
    assert_eq!(frame.overlay_batches.len(), 1);
    assert_eq!(frame.stats.quads, 2);
}

#[test]
fn file_permission_and_link_emblems_use_overlay_layer() {
    assert_eq!(icon_emblem_draw_layer(), IconDrawLayer::Overlay);
}

#[test]
fn file_emblems_snap_to_physical_pixels() {
    let rects = icon_emblem_rects(
        ViewRect {
            x: 10.25,
            y: 20.75,
            width: 30.0,
            height: 30.0,
        },
        1.25,
    );

    for rect in rects {
        assert_eq!(rect.x.fract(), 0.0);
        assert_eq!(rect.y.fract(), 0.0);
        assert_eq!(rect.width.fract(), 0.0);
        assert_eq!(rect.height.fract(), 0.0);
    }
}

#[test]
fn named_overlay_icon_becomes_encoded_gpu_source_after_theme_resolution() {
    let mut harness = FileIconResolverTestHarness::new();
    let mut thumbnails = ThumbnailSourceResolver::new();
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
    let request_key = harness.next_request_key().expect("theme resolve queued");
    let resolved_path = PathBuf::from("/theme/actions/archive-insert.svg");
    harness.complete(request_key, Some(resolved_path.clone()));

    let frame = {
        let mut builder = IconFrameBuilder::new_for_test(
            &mut harness.resolver,
            &mut thumbnails,
            PhysicalSize::new(128, 96),
        );
        assert!(builder.push_named_theme_icon(
            "archive-insert",
            NamedIconFallback::Service,
            icon,
            clip,
            IconDrawLayer::Overlay,
        ));
        builder.finish()
    };
    assert_eq!(frame.slots.len(), 1);
    assert_eq!(
        frame.slots[0]
            .source
            .as_ref()
            .and_then(IconGpuSource::file_path),
        Some(resolved_path.as_path())
    );
}

#[test]
fn scrolling_icon_miss_uses_preliminary_icon_and_queues_exact_role() {
    let mut harness = FileIconResolverTestHarness::new();
    let mut thumbnails = ThumbnailSourceResolver::new();
    let preliminary_role = crate::ui::icon_roles::FileIconRoleCacheKey {
        kind: FileIconKind::PreliminaryFile { extension: None },
    };
    let fallback_path = PathBuf::from("/theme/mimetypes/text-x-generic.svg");
    harness.complete(
        crate::ui::icon_roles::FileIconPathCacheKey {
            role: preliminary_role.clone(),
            size_px: 32,
        },
        Some(fallback_path.clone()),
    );
    let entry = test_unchecked_generic_entry("cold.zzz", 1, 17);

    let frame = {
        let mut builder = IconFrameBuilder::new(
            IconFrameResources::new(
                &mut harness.resolver,
                &mut thumbnails,
                IconGpuResidentIndex::default(),
            ),
            IconFrameConfig::new(PhysicalSize::new(128, 96), 1.0, 0),
        );
        assert!(builder.push_icon(
            Path::new("/tmp"),
            &entry,
            ViewRect {
                x: 4.0,
                y: 4.0,
                width: 28.0,
                height: 28.0,
            },
            ViewRect {
                x: 0.0,
                y: 0.0,
                width: 128.0,
                height: 96.0,
            },
            IconDrawLayer::Content,
        ));
        builder.finish()
    };

    assert_eq!(frame.stats.deferred, 1);
    assert_eq!(frame.stats.fallbacks, 0);
    assert_eq!(frame.slots.len(), 1);
    assert_eq!(
        frame.slots[0]
            .source
            .as_ref()
            .and_then(IconGpuSource::file_path),
        Some(fallback_path.as_path())
    );
    assert_eq!(
        frame.slots[0].identity,
        IconGpuUploadKey::role(preliminary_role.kind, 32)
    );
    let exact = harness.next_request_key().expect("exact role queued");
    assert_eq!(
        exact.role.kind,
        FileIconKind::PreliminaryFile {
            extension: Some("zzz".to_string())
        }
    );
}

#[test]
fn gpu_resident_thumbnail_does_not_requeue_encoded_source_work() {
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
                rounding: None,
            },
        )]),
    };
    let mut resolver = FileIconResolver::new();
    let mut thumbnails = ThumbnailSourceResolver::new();
    let entry =
        test_entry_with_mime_and_modified("resident.png", false, "image/png", Some(modified_secs));
    let mut builder = IconFrameBuilder::new(
        IconFrameResources::new(&mut resolver, &mut thumbnails, resident),
        IconFrameConfig::new(PhysicalSize::new(128, 96), 1.0, 0),
    );

    assert!(builder.push_thumbnail(
        &directory,
        &entry,
        ViewRect {
            x: 4.0,
            y: 4.0,
            width: 48.0,
            height: 48.0
        },
        ViewRect {
            x: 0.0,
            y: 0.0,
            width: 128.0,
            height: 96.0
        },
        IconDrawLayer::Content,
    ));
    assert!(thumbnails.pending.is_empty());
}

#[test]
fn active_zoom_rasterizes_known_mime_at_target_size_without_preliminary_replacement() {
    let entry = test_entry_with_mime_and_modified("document.txt", false, "text/plain", Some(17));
    let role = FileIconKind::Mime {
        mime: Arc::from("text/plain"),
    };
    let old_gpu_key = IconGpuUploadKey::role(role.clone(), 48);
    let target_gpu_key = IconGpuUploadKey::role(role, 128);
    let resident = IconGpuResidentIndex {
        entries: HashMap::from([(
            old_gpu_key,
            IconGpuResidentEntry {
                width: 48,
                height: 48,
                content_width: 48,
                content_height: 48,
                content_hash: 0x1234,
                rounding: None,
            },
        )]),
    };
    let mut resolver = FileIconResolver::new();
    let mut thumbnails = ThumbnailSourceResolver::new();
    let mut config = IconFrameConfig::new(PhysicalSize::new(256, 192), 1.0, 0);
    config.role_updates_paused = true;
    config.icon_size_update_pending = true;
    let mut builder = IconFrameBuilder::new(
        IconFrameResources::new(&mut resolver, &mut thumbnails, resident),
        config,
    );

    assert!(builder.push_icon(
        Path::new("/tmp"),
        &entry,
        ViewRect {
            x: 8.0,
            y: 8.0,
            width: 128.0,
            height: 128.0,
        },
        ViewRect {
            x: 0.0,
            y: 0.0,
            width: 256.0,
            height: 192.0,
        },
        IconDrawLayer::Content,
    ));

    let frame = builder.finish();
    assert_eq!(frame.slots.len(), 1);
    assert_eq!(frame.slots[0].identity, target_gpu_key);
    assert_eq!(frame.slots[0].content_width, 128);
    assert_eq!(frame.slots[0].source.as_ref().map(IconGpuSource::size_px), Some(128));
    assert_eq!(frame.stats.cache_hits, 0);
    assert_eq!(frame.stats.cache_misses, 1);
    assert_eq!(frame.stats.quads, 1);
}

#[test]
fn active_zoom_scales_small_resident_thumbnail_without_requesting_an_upgrade() {
    let directory = PathBuf::from("/tmp");
    let path = directory.join("resident.png");
    let modified_secs = 17;
    let gpu_key = IconGpuUploadKey::content(path, modified_secs);
    let resident = IconGpuResidentIndex {
        entries: HashMap::from([(
            gpu_key.clone(),
            IconGpuResidentEntry {
                width: 48,
                height: 48,
                content_width: 48,
                content_height: 48,
                content_hash: 0x5678,
                rounding: None,
            },
        )]),
    };
    let mut resolver = FileIconResolver::new();
    let mut thumbnails = ThumbnailSourceResolver::new();
    let entry =
        test_entry_with_mime_and_modified("resident.png", false, "image/png", Some(modified_secs));
    let mut config = IconFrameConfig::new(PhysicalSize::new(256, 192), 1.0, 0);
    config.role_updates_paused = true;
    config.icon_size_update_pending = true;
    let mut builder = IconFrameBuilder::new(
        IconFrameResources::new(&mut resolver, &mut thumbnails, resident),
        config,
    );

    assert!(builder.push_thumbnail(
        &directory,
        &entry,
        ViewRect {
            x: 8.0,
            y: 8.0,
            width: 128.0,
            height: 128.0,
        },
        ViewRect {
            x: 0.0,
            y: 0.0,
            width: 256.0,
            height: 192.0,
        },
        IconDrawLayer::Content,
    ));

    let frame = builder.finish();
    assert!(thumbnails.pending.is_empty());
    assert_eq!(frame.slots.len(), 1);
    assert_eq!(frame.slots[0].identity, gpu_key);
    assert_eq!(frame.slots[0].content_hash, 0x5678);
    assert_eq!(frame.slots[0].content_width, 48);
    assert!(frame.slots[0].source.is_none());
    assert_eq!(frame.stats.cache_hits, 1);
    assert_eq!(frame.stats.cache_misses, 0);
    assert_eq!(frame.stats.thumbnail_quads, 1);
}

#[test]
fn active_zoom_reuses_exact_size_resident_emblem() {
    let gpu_key = IconGpuUploadKey::named_asset("emblem-readonly".to_string(), 32);
    let resident = IconGpuResidentIndex {
        entries: HashMap::from([(
            gpu_key.clone(),
            IconGpuResidentEntry {
                width: 32,
                height: 32,
                content_width: 32,
                content_height: 32,
                content_hash: 0x9abc,
                rounding: None,
            },
        )]),
    };
    let mut resolver = FileIconResolver::new();
    let mut thumbnails = ThumbnailSourceResolver::new();
    let mut config = IconFrameConfig::new(PhysicalSize::new(128, 96), 1.0, 0);
    config.role_updates_paused = true;
    config.icon_size_update_pending = true;
    let mut builder = IconFrameBuilder::new(
        IconFrameResources::new(&mut resolver, &mut thumbnails, resident),
        config,
    );
    let rect = ViewRect {
        x: 8.0,
        y: 8.0,
        width: 32.0,
        height: 32.0,
    };

    assert!(builder.push_named_theme_icon_exact(
        "emblem-readonly",
        rect,
        rect,
        IconDrawLayer::Overlay,
    ));

    let frame = builder.finish();
    assert_eq!(frame.slots.len(), 1);
    assert_eq!(frame.slots[0].identity, gpu_key);
    assert_eq!(frame.slots[0].content_hash, 0x9abc);
    assert_eq!(frame.slots[0].content_width, 32);
    assert!(frame.slots[0].source.is_none());
    assert_eq!(frame.stats.cache_hits, 1);
    assert_eq!(frame.stats.cache_misses, 0);
    assert_eq!(frame.overlay_vertices.len(), 6);
}
