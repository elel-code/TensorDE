
    #[test]
    fn folder_preview_gpu_draw_rect_stays_inside_folder_shell_slot() {
        let layout = ItemPixmapLayout {
            view_mode: ShellViewMode::Icons,
            icon_rect: ViewRect {
                x: 50.0,
                y: 20.0,
                width: 100.0,
                height: 100.0,
            },
            text_rect: ViewRect {
                x: 12.0,
                y: 124.0,
                width: 176.0,
                height: 16.0,
            },
            text_midline_shift: 0.0,
        };
        let shell = folder_preview_role_shell_rect(layout);
        let slot = folder_preview_role_slot(layout);
        let draw = folder_preview_gpu_draw_rect(layout, 80);

        assert!(slot.x >= shell.x);
        assert!(slot.y >= shell.y);
        assert!(slot.right() <= shell.right() + f32::EPSILON);
        assert!(slot.bottom() <= shell.bottom() + f32::EPSILON);
        assert!(draw.width <= slot.width + f32::EPSILON);
        assert!(draw.height <= slot.height + f32::EPSILON);
        assert!((draw.width - draw.height).abs() < 0.05);
        assert!((draw.x + draw.width / 2.0 - (slot.x + slot.width / 2.0)).abs() < f32::EPSILON);
        assert!((draw.y + draw.height / 2.0 - (slot.y + slot.height / 2.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn folder_preview_gpu_draw_rect_uses_file_manager_text_midline_shift_in_compact_area() {
        let layout = ItemPixmapLayout {
            view_mode: ShellViewMode::Compact,
            icon_rect: ViewRect {
                x: 4.0,
                y: 8.0,
                width: 48.0,
                height: 48.0,
            },
            text_rect: ViewRect {
                x: 60.0,
                y: 22.0,
                width: 140.0,
                height: 18.0,
            },
            text_midline_shift: 3.0,
        };
        let area = folder_preview_role_shell_rect(layout);
        let draw = folder_preview_gpu_draw_rect(layout, 48);
        let expected_center_y = layout.text_rect.y + layout.text_rect.height / 2.0 + 3.0;

        assert!((area.y + area.height / 2.0 - expected_center_y).abs() < f32::EPSILON);
        assert!(draw.y >= area.y);
        assert!(draw.bottom() <= area.bottom() + f32::EPSILON);
        assert!((draw.x + draw.width / 2.0 - (area.x + area.width / 2.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn file_manager_text_midline_shift_matches_font_metrics_formula() {
        let shift = file_manager_text_midline_shift_from_metrics(18.0, 14.0, 1000, -200.0, Some(700.0));

        assert!((shift - 1.3).abs() < 0.01);
    }

    #[test]
    fn folder_preview_role_dirty_key_tracks_role_state_but_not_hoverless_hover() {
        let root = test_dir("directory-preview-dirty-key");
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
        let size = PhysicalSize::new(700, 320);
        let before = ShellRenderDirtyKey::from_scene(&scene, size);
        scene.folder_preview_roles.borrow_mut().insert_ready(
            FolderPreviewRoleKey::new(album, 7, 48),
            FolderPreviewReady {
                stamp: 11,
                size_px: 48,
                source: test_folder_preview_source(test_gpu_icon_spec(2, 3)),
            },
        );
        let after = ShellRenderDirtyKey::from_scene(&scene, size);
        assert_ne!(before, after);

        let hoverless_before = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);
        scene.set_hovered_item(Some(ShellPaneItemTarget {
            pane: ShellPaneId::SLOT_0,
            index: 0,
        }));
        let hoverless_after = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);
        assert_eq!(hoverless_before, hoverless_after);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn folder_preview_role_dirty_key_ignores_offscreen_ready_entries() {
        let root = test_dir("directory-preview-visible-dirty-key");
        let album = root.join("album");
        let offscreen = root.join("offscreen");
        fs::create_dir_all(&album).unwrap();
        fs::create_dir_all(&offscreen).unwrap();
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
        let size = PhysicalSize::new(700, 320);
        let before = ShellRenderDirtyKey::from_scene(&scene, size);

        scene.folder_preview_roles.borrow_mut().insert_ready(
            FolderPreviewRoleKey::new(offscreen, 7, 48),
            FolderPreviewReady {
                stamp: 11,
                size_px: 48,
                source: test_folder_preview_source(test_gpu_icon_spec(2, 3)),
            },
        );
        let offscreen_after = ShellRenderDirtyKey::from_scene(&scene, size);
        assert_eq!(before, offscreen_after);

        scene.folder_preview_roles.borrow_mut().insert_ready(
            FolderPreviewRoleKey::new(album, 7, 48),
            FolderPreviewReady {
                stamp: 12,
                size_px: 48,
                source: test_folder_preview_source(test_gpu_icon_spec(2, 4)),
            },
        );
        let visible_after = ShellRenderDirtyKey::from_scene(&scene, size);
        assert_ne!(before, visible_after);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn folder_preview_role_dirty_key_ignores_unused_ready_size_variant() {
        let root = test_dir("directory-preview-unused-size-dirty-key");
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
        let size = PhysicalSize::new(700, 320);
        scene.folder_preview_roles.borrow_mut().insert_ready(
            FolderPreviewRoleKey::new(album.clone(), 7, 128),
            FolderPreviewReady {
                stamp: 11,
                size_px: 128,
                source: test_folder_preview_source(test_gpu_icon_spec(2, 3)),
            },
        );
        let exact_before = ShellRenderDirtyKey::from_scene(&scene, size);

        scene.folder_preview_roles.borrow_mut().insert_ready(
            FolderPreviewRoleKey::new(album.clone(), 7, 256),
            FolderPreviewReady {
                stamp: 12,
                size_px: 256,
                source: test_folder_preview_source(test_gpu_icon_spec(2, 4)),
            },
        );
        let exact_after = ShellRenderDirtyKey::from_scene(&scene, size);
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let rects = folder_preview_damage_rects_for_changed_keys(
            &scene,
            &projections,
            &[FolderPreviewRoleKey::new(album.clone(), 7, 256)],
        );

        assert_eq!(exact_before, exact_after);
        assert!(rects.is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn folder_preview_role_dirty_key_tracks_closest_ready_size_when_exact_is_missing() {
        let root = test_dir("directory-preview-closest-size-dirty-key");
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
        let size = PhysicalSize::new(700, 320);
        let before = ShellRenderDirtyKey::from_scene(&scene, size);

        scene.folder_preview_roles.borrow_mut().insert_ready(
            FolderPreviewRoleKey::new(album.clone(), 7, 256),
            FolderPreviewReady {
                stamp: 12,
                size_px: 256,
                source: test_folder_preview_source(test_gpu_icon_spec(2, 4)),
            },
        );
        let after = ShellRenderDirtyKey::from_scene(&scene, size);
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let rects = folder_preview_damage_rects_for_changed_keys(
            &scene,
            &projections,
            &[FolderPreviewRoleKey::new(album.clone(), 7, 256)],
        );

        assert_ne!(before, after);
        assert_eq!(rects.len(), 1);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn folder_preview_damage_covers_folder_shell_when_preview_changes() {
        let root = test_dir("directory-preview-replacement-damage");
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
        let size = PhysicalSize::new(700, 320);
        let key = FolderPreviewRoleKey::new(album.clone(), 7, 128);
        let previous = FolderPreviewReady {
            stamp: 11,
            size_px: 128,
            source: test_folder_preview_source(solid_gpu_icon_spec(96, 24, [220, 40, 80, 255])),
        };
        let current = FolderPreviewReady {
            stamp: 12,
            size_px: 128,
            source: test_folder_preview_source(solid_gpu_icon_spec(24, 96, [40, 120, 220, 255])),
        };
        scene
            .folder_preview_roles
            .borrow_mut()
            .insert_ready(key.clone(), current.clone());
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let item = projections[0].visible_items[0];
        let layout = ItemPixmapLayout::from_item_layout(projections[0].view.view_mode, item.layout);
        let expected =
            pane_content_rect_to_screen(folder_preview_role_shell_rect(layout), &projections[0]);

        let rects = folder_preview_damage_rects_for_changes(
            &scene,
            &projections,
            &[FolderPreviewRoleChange {
                key,
                previous: Some(previous.clone()),
            }],
        );

        assert_eq!(rects, vec![expected]);
        let previous_draw = pane_content_rect_to_screen(
            folder_preview_gpu_draw_rect(layout, previous.size_px),
            &projections[0],
        );
        let current_draw = pane_content_rect_to_screen(
            folder_preview_gpu_draw_rect(layout, current.size_px),
            &projections[0],
        );
        assert!(expected.x <= previous_draw.x);
        assert!(expected.y <= previous_draw.y);
        assert!(expected.right() >= previous_draw.right());
        assert!(expected.bottom() >= previous_draw.bottom());
        assert!(expected.x <= current_draw.x);
        assert!(expected.y <= current_draw.y);
        assert!(expected.right() >= current_draw.right());
        assert!(expected.bottom() >= current_draw.bottom());
        assert!(
            rect_area(expected)
                < rect_area(pane_content_rect_to_screen(
                    item.layout.visual_rect,
                    &projections[0]
                ))
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn folder_preview_damage_ignores_replaced_unused_ready_size_variant() {
        let root = test_dir("directory-preview-unused-replacement-damage");
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
        let size = PhysicalSize::new(700, 320);
        let exact_key = FolderPreviewRoleKey::new(album.clone(), 7, 128);
        let unused_key = FolderPreviewRoleKey::new(album.clone(), 7, 256);
        scene.folder_preview_roles.borrow_mut().insert_ready(
            exact_key,
            FolderPreviewReady {
                stamp: 11,
                size_px: 128,
                source: test_folder_preview_source(solid_gpu_icon_spec(48, 48, [40, 120, 220, 255])),
            },
        );
        scene.folder_preview_roles.borrow_mut().insert_ready(
            unused_key.clone(),
            FolderPreviewReady {
                stamp: 12,
                size_px: 256,
                source: test_folder_preview_source(solid_gpu_icon_spec(96, 24, [220, 40, 80, 255])),
            },
        );
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();

        let rects = folder_preview_damage_rects_for_changes(
            &scene,
            &projections,
            &[FolderPreviewRoleChange {
                key: unused_key,
                previous: Some(FolderPreviewReady {
                    stamp: 10,
                    size_px: 256,
                    source: test_folder_preview_source(solid_gpu_icon_spec(24, 96, [80, 220, 40, 255])),
                }),
            }],
        );

        assert!(rects.is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn folder_preview_damage_covers_previous_preview_when_role_fails() {
        let root = test_dir("directory-preview-failure-replacement-damage");
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
        let size = PhysicalSize::new(700, 320);
        let key = FolderPreviewRoleKey::new(album.clone(), 7, 128);
        let previous = FolderPreviewReady {
            stamp: 11,
            size_px: 128,
            source: test_folder_preview_source(solid_gpu_icon_spec(96, 24, [220, 40, 80, 255])),
        };
        {
            let mut roles = scene.folder_preview_roles.borrow_mut();
            roles.failed.insert(key.clone());
            roles.finished.insert(key.clone());
        }
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let item = projections[0].visible_items[0];
        let layout = ItemPixmapLayout::from_item_layout(projections[0].view.view_mode, item.layout);
        let expected =
            pane_content_rect_to_screen(folder_preview_role_shell_rect(layout), &projections[0]);

        let rects = folder_preview_damage_rects_for_changes(
            &scene,
            &projections,
            &[FolderPreviewRoleChange {
                key,
                previous: Some(previous.clone()),
            }],
        );

        assert_eq!(rects, vec![expected]);
        let previous_draw = pane_content_rect_to_screen(
            folder_preview_gpu_draw_rect(layout, previous.size_px),
            &projections[0],
        );
        assert!(expected.x <= previous_draw.x);
        assert!(expected.y <= previous_draw.y);
        assert!(expected.right() >= previous_draw.right());
        assert!(expected.bottom() >= previous_draw.bottom());

        let _ = fs::remove_dir_all(root);
    }
