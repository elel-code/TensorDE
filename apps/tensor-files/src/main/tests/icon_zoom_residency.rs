fn build_text_icon_zoom_frame(
    resolver: &mut FileIconResolver,
    thumbnails: &mut ThumbnailSourceResolver,
    resident: IconGpuResidentIndex,
    icon_size_update_pending: bool,
) -> IconFrame {
    let entry = test_entry_with_mime_and_modified("document.txt", false, "text/plain", Some(17));
    let mut config = IconFrameConfig::new(
        PhysicalSize::new(256, 192),
        1.0,
        usize::from(!icon_size_update_pending),
    );
    config.role_updates_paused = icon_size_update_pending;
    config.icon_size_update_pending = icon_size_update_pending;
    let mut builder = IconFrameBuilder::new(
        IconFrameResources::new(resolver, thumbnails, resident),
        config,
    );
    assert!(builder.push_icon(
        Path::new("/tmp/document.txt"),
        &entry,
        None,
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
    builder.finish()
}

fn text_icon_target_source(resolver: &mut FileIconResolver) -> IconGpuSource {
    let entry = test_entry_with_mime_and_modified("document.txt", false, "text/plain", Some(17));
    let exact = resolver
        .resolve_entry_visible_fast(Path::new("/tmp"), &entry, 128.0)
        .path
        .expect("text/plain theme icon must resolve");
    IconGpuSource::file(exact, 128)
}

fn text_icon_resident(content_hash: u64) -> IconGpuResidentIndex {
    let key = IconGpuUploadKey::role(
        FileIconKind::Mime {
            mime: Arc::from("text/plain"),
        },
        128,
    );
    IconGpuResidentIndex {
        entries: HashMap::from([(
            key,
            IconGpuResidentEntry {
                width: 128,
                height: 128,
                content_width: 128,
                content_height: 128,
                content_hash,
                rounding: None,
            },
        )]),
    }
}

#[test]
fn active_zoom_reuses_exact_size_resident_mime_without_upload() {
    let mut resolver = FileIconResolver::new();
    let source = text_icon_target_source(&mut resolver);
    let mut thumbnails = ThumbnailSourceResolver::new();
    let frame = build_text_icon_zoom_frame(
        &mut resolver,
        &mut thumbnails,
        text_icon_resident(source.content_hash()),
        true,
    );

    assert_eq!(frame.slots.len(), 1);
    assert!(frame.slots[0].source.is_none());
    assert_eq!(frame.stats.cache_hits, 1);
    assert_eq!(frame.stats.cache_misses, 0);
    assert_eq!(frame.stats.quads, 1);
}

#[test]
fn theme_asset_key_and_file_source_share_path_storage() {
    let path = Arc::<Path>::from(Path::new("/usr/share/icons/folder.svg"));
    let key = IconGpuUploadKey::theme_asset(Arc::clone(&path), 64);
    let source = IconGpuSource::file(Arc::clone(&path), 64);

    let IconGpuIdentity::ThemeAsset {
        path: key_path, ..
    } = &key.identity
    else {
        panic!("theme asset key should retain theme path");
    };
    let IconGpuSource::File {
        path: source_path, ..
    } = &source
    else {
        panic!("file source should retain theme path");
    };
    assert!(Arc::ptr_eq(&path, key_path));
    assert!(Arc::ptr_eq(&path, source_path));
}

#[test]
fn zoom_and_settle_keep_identical_item_icon_geometry() {
    let mut resolver = FileIconResolver::new();
    let _ = text_icon_target_source(&mut resolver);
    let mut thumbnails = ThumbnailSourceResolver::new();
    let active = build_text_icon_zoom_frame(
        &mut resolver,
        &mut thumbnails,
        text_icon_resident(0xdead),
        true,
    );
    let active_slot = &active.slots[0];
    let exact_resident = text_icon_resident(active_slot.content_hash);
    let settled = build_text_icon_zoom_frame(
        &mut resolver,
        &mut thumbnails,
        exact_resident,
        false,
    );

    assert!(active_slot.source.is_some());
    assert!(settled.slots[0].source.is_none());
    assert_eq!(
        bytemuck::cast_slice::<IconVertex, u8>(&active.content_vertices),
        bytemuck::cast_slice::<IconVertex, u8>(&settled.content_vertices)
    );
}

#[test]
fn folder_preview_source_upgrade_keeps_target_geometry_stable() {
    let mut resolver = FileIconResolver::new();
    let mut thumbnails = ThumbnailSourceResolver::new();
    let children = Arc::<[PathBuf]>::from([PathBuf::from("/tmp/preview.png")]);
    let layout = ItemPixmapLayout {
        view_mode: ShellViewMode::Icons,
        icon_rect: ViewRect {
            x: 12.0,
            y: 20.0,
            width: 192.0,
            height: 192.0,
        },
        text_rect: ViewRect {
            x: 12.0,
            y: 216.0,
            width: 192.0,
            height: 18.0,
        },
        text_midline_shift: 0.0,
    };
    let entry = test_entry_with_mime_and_modified(
        "folder",
        true,
        "inode/directory",
        Some(17),
    );
    let folder_path = Arc::<Path>::from(Path::new("/tmp/folder"));

    let mut active = IconFrameBuilder::new_for_test(
        &mut resolver,
        &mut thumbnails,
        PhysicalSize::new(256, 192),
    );
    assert!(active.push_thumbnail_or_icon_with_shared_path(
        &folder_path,
        &entry,
        None,
        Some(&FolderPreviewReady {
            stamp: 17,
            size_px: 128,
            source: IconGpuSource::FolderPreview {
                children: Arc::clone(&children),
                size_px: 128,
                seed: 7,
            },
        }),
        layout,
        ViewRect {
            x: 0.0,
            y: 0.0,
            width: 256.0,
            height: 256.0,
        },
    ));
    let active = active.finish();

    let mut settled = IconFrameBuilder::new_for_test(
        &mut resolver,
        &mut thumbnails,
        PhysicalSize::new(256, 192),
    );
    assert!(settled.push_thumbnail_or_icon_with_shared_path(
        &folder_path,
        &entry,
        None,
        Some(&FolderPreviewReady {
            stamp: 17,
            size_px: 256,
            source: IconGpuSource::FolderPreview {
                children,
                size_px: 256,
                seed: 7,
            },
        }),
        layout,
        ViewRect {
            x: 0.0,
            y: 0.0,
            width: 256.0,
            height: 256.0,
        },
    ));
    let settled = settled.finish();

    assert_eq!(active.draws[1].screen, settled.draws[1].screen);
    assert_eq!(active.draws[1].screen, layout.icon_rect);
    for frame in [&active, &settled] {
        let content_path = frame
            .slots
            .iter()
            .find_map(|slot| match &slot.identity.identity {
                IconGpuIdentity::Content { path, .. } => Some(path),
                _ => None,
            })
            .expect("folder preview content identity");
        assert!(Arc::ptr_eq(content_path, &folder_path));
    }
}

#[test]
fn native_icon_path_prefers_retained_remote_target_and_falls_back_once() {
    let target = PathBuf::from("smb://server/share/document.txt");
    let remote = test_entry_with_target("document.txt", false, target.clone());
    let mut slots = ShellPaneVisibleSlotPools::default();
    slots.update_visible_items(ShellPaneId::SLOT_0, [target.as_path()]);

    let retained = icon_path_for_entry(
        &slots,
        ShellPaneId::SLOT_0,
        Path::new("/tmp"),
        &remote,
    );
    let retained_slot = slots
        .get(ShellPaneId::SLOT_0)
        .retained_shared_path_for_entry(&remote)
        .expect("retained visible path");
    assert!(Arc::ptr_eq(&retained, &retained_slot));
    assert_eq!(retained.as_ref(), target.as_path());

    let local = test_entry("document.txt", false);
    let fallback = icon_path_for_entry(
        &slots,
        ShellPaneId::SLOT_0,
        Path::new("/tmp/downloads"),
        &local,
    );
    assert_eq!(fallback.as_ref(), Path::new("/tmp/downloads/document.txt"));
}
