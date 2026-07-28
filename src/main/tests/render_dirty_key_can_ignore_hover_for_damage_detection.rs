
    #[test]
    fn render_dirty_key_can_ignore_hover_for_damage_detection() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let initial = ShellRenderDirtyKey::from_scene(&scene, size);
        let initial_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        assert!(scene.set_hovered_item(Some(ShellPaneItemTarget {
            pane: ShellPaneId::SLOT_0,
            index: 0,
        })));
        let hovered = ShellRenderDirtyKey::from_scene(&scene, size);
        let hovered_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        assert_ne!(initial, hovered);
        assert_eq!(initial_hoverless, hovered_hoverless);
    }

    #[test]
    fn render_dirty_key_can_ignore_rubber_band_for_damage_detection() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let initial = ShellRenderDirtyKey::from_scene(&scene, size);
        let initial_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        scene.rubber_band = Some(RubberBand {
            start: ViewPoint { x: 10.0, y: 12.0 },
            current: ViewPoint { x: 50.0, y: 54.0 },
            active: true,
            mode: RubberBandMode::Replace,
            base_selection: scene.panes[ShellPaneId::SLOT_0].selection.clone(),
        });
        scene.rubber_band_updates += 1;
        let dragged = ShellRenderDirtyKey::from_scene(&scene, size);
        let dragged_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        assert_ne!(initial, dragged);
        assert_eq!(initial_hoverless, dragged_hoverless);
    }

    #[test]
    fn render_damage_bounds_hover_item_transition() {
        let mut scene = test_scene(
            vec![
                test_entry("alpha.txt", false),
                test_entry("beta.txt", false),
            ],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(700, 320);
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let initial = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        assert!(scene.set_hovered_item(Some(ShellPaneItemTarget {
            pane: ShellPaneId::SLOT_0,
            index: 0,
        })));
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let hovered = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        let damage = ShellRenderDamage::between(Some(&initial), &hovered, false);

        assert_eq!(damage.kind, ShellRenderDamageKind::Bounded);
        assert_eq!(damage.rect_count, 1);
        assert!(damage.area_px > 0.0);
        assert!(damage.area_px < rect_area(full_surface_rect(size)));
    }

    #[test]
    fn render_damage_bounds_visible_folder_preview_role_result() {
        let scene = test_scene(
            vec![test_entry_with_mime_and_modified(
                "album",
                true,
                "inode/directory",
                Some(7),
            )],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(700, 320);
        let album = PathBuf::from("/tmp/album");
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let initial = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        scene.folder_preview_roles.borrow_mut().insert_ready(
            FolderPreviewRoleKey::new(album.clone(), 7, 48),
            FolderPreviewReady {
                stamp: 11,
                size_px: 48,
                source: test_folder_preview_source(test_gpu_icon_spec(2, 3)),
            },
        );
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let changed = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );
        let rects = folder_preview_damage_rects_for_changed_keys(
            &scene,
            &projections,
            &[FolderPreviewRoleKey::new(album.clone(), 7, 48)],
        );
        let item = projections[0].visible_items[0];
        let expected_rect = pane_content_rect_to_screen(
            folder_preview_role_shell_rect(ItemPixmapLayout::from_item_layout(
                projections[0].view.view_mode,
                item.layout,
            )),
            &projections[0],
        );

        let damage =
            ShellRenderDamage::between_with_async_damage(Some(&initial), &changed, false, rects);

        assert_eq!(damage.kind, ShellRenderDamageKind::Bounded);
        assert_eq!(damage.rect_count, 1);
        assert_eq!(damage.bounds, Some(expected_rect));
        assert!(damage.area_px > 0.0);
        assert!(
            damage.area_px
                < rect_area(pane_content_rect_to_screen(
                    item.layout.visual_rect,
                    &projections[0]
                ))
        );
        assert!(damage.area_px < rect_area(full_surface_rect(size)));
    }

    #[test]
    fn render_damage_bounds_folder_preview_role_result_with_hover_transition() {
        let mut scene = test_scene(
            vec![test_entry_with_mime_and_modified(
                "album",
                true,
                "inode/directory",
                Some(7),
            )],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(700, 320);
        let album = PathBuf::from("/tmp/album");
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let initial = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        scene.folder_preview_roles.borrow_mut().insert_ready(
            FolderPreviewRoleKey::new(album.clone(), 7, 48),
            FolderPreviewReady {
                stamp: 11,
                size_px: 48,
                source: test_folder_preview_source(test_gpu_icon_spec(2, 3)),
            },
        );
        assert!(scene.set_hovered_item(Some(ShellPaneItemTarget {
            pane: ShellPaneId::SLOT_0,
            index: 0,
        })));
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let changed = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );
        let rects = folder_preview_damage_rects_for_changed_keys(
            &scene,
            &projections,
            &[FolderPreviewRoleKey::new(album.clone(), 7, 48)],
        );

        let damage =
            ShellRenderDamage::between_with_async_damage(Some(&initial), &changed, false, rects);

        assert_eq!(damage.kind, ShellRenderDamageKind::Bounded);
        assert!(damage.rect_count >= 1);
        assert!(damage.area_px > 0.0);
        assert!(damage.area_px < rect_area(full_surface_rect(size)));
    }

    #[test]
    fn render_damage_is_clean_for_offscreen_folder_preview_role_result() {
        let scene = test_scene(
            vec![test_entry_with_mime_and_modified(
                "album",
                true,
                "inode/directory",
                Some(7),
            )],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(700, 320);
        let offscreen = PathBuf::from("/tmp/offscreen");
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let initial = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        scene.folder_preview_roles.borrow_mut().insert_ready(
            FolderPreviewRoleKey::new(offscreen.clone(), 7, 48),
            FolderPreviewReady {
                stamp: 11,
                size_px: 48,
                source: test_folder_preview_source(test_gpu_icon_spec(2, 3)),
            },
        );
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let changed = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );
        let rects = folder_preview_damage_rects_for_changed_keys(
            &scene,
            &projections,
            &[FolderPreviewRoleKey::new(offscreen.clone(), 7, 48)],
        );

        let damage =
            ShellRenderDamage::between_with_async_damage(Some(&initial), &changed, false, rects);

        assert_eq!(damage.kind, ShellRenderDamageKind::Clean);
    }

    #[test]
    fn render_damage_bounds_rubber_band_transition() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        scene.rubber_band = Some(RubberBand {
            start: ViewPoint { x: 10.0, y: 12.0 },
            current: ViewPoint { x: 46.0, y: 52.0 },
            active: true,
            mode: RubberBandMode::Replace,
            base_selection: scene.panes[ShellPaneId::SLOT_0].selection.clone(),
        });
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let initial = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        scene.rubber_band.as_mut().unwrap().current = ViewPoint { x: 72.0, y: 80.0 };
        scene.rubber_band_updates += 1;
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let dragged = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        let damage = ShellRenderDamage::between(Some(&initial), &dragged, false);
        let bounds = damage.bounds.unwrap();
        let previous_rect = initial.rubber_band_rect.unwrap();
        let current_rect = dragged.rubber_band_rect.unwrap();

        assert_eq!(damage.kind, ShellRenderDamageKind::Bounded);
        assert_eq!(damage.rect_count, 2);
        assert!(damage.area_px > 0.0);
        assert!(damage.area_px < rect_area(full_surface_rect(size)));
        assert!(bounds.x <= previous_rect.x);
        assert!(bounds.y <= previous_rect.y);
        assert!(bounds.right() >= current_rect.right());
        assert!(bounds.bottom() >= current_rect.bottom());
    }

    #[test]
    fn render_damage_bounds_ignore_internal_drag_pointer_motion() {
        // No in-window drag overlay: pointer motion during an internal drag
        // should not produce window damage by itself.
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        scene.internal_drag = Some(ShellInternalDrag {
            source: ShellInternalDragSource::PaneItem {
                pane: ShellPaneId::SLOT_0,
                index: 0,
                source_path: PathBuf::from("/tmp/alpha.txt"),
                is_dir: false,
            },
            paths: vec![PathBuf::from("/tmp/alpha.txt")],
            label: "alpha.txt".to_string(),
            start: ViewPoint { x: 10.0, y: 12.0 },
            current: ViewPoint { x: 48.0, y: 54.0 },
            active: true,
        });
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let initial = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        scene.internal_drag.as_mut().unwrap().current = ViewPoint { x: 92.0, y: 96.0 };
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let moved = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        let damage = ShellRenderDamage::between(Some(&initial), &moved, false);
        assert_eq!(damage.kind, ShellRenderDamageKind::Clean);
        assert!(damage.bounds.is_none());
    }

    #[test]
    fn render_damage_is_clean_for_external_drag_lifecycle_without_visible_target() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let initial = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        scene.external_drag = Some(ShellExternalDrag {
            sources: vec![PathBuf::from("/tmp/source.txt")],
            local_source: None,
        });
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let dragging = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        let damage = ShellRenderDamage::between(Some(&initial), &dragging, false);

        assert_eq!(damage.kind, ShellRenderDamageKind::Clean);
    }

    #[test]
    fn render_damage_bounds_places_scroll_transition() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        scene.places = (0..24)
            .map(|index| {
                ShellPlace::new(
                    "",
                    "B",
                    format!("Place {index:02}"),
                    PathBuf::from(format!("/tmp/place-{index:02}")),
                    true,
                )
            })
            .collect();
        let size = PhysicalSize::new(700, 180);
        assert!(scene.max_places_scroll_y(size) > 0.0);
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let initial = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        assert!(scene.scroll_places_by(48.0, size));
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let scrolled = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        let damage = ShellRenderDamage::between(Some(&initial), &scrolled, false);

        assert_eq!(damage.kind, ShellRenderDamageKind::Bounded);
        assert_eq!(damage.rect_count, 1);
        assert_eq!(damage.bounds, scrolled.places_sidebar_rect);
        assert!(damage.area_px < rect_area(full_surface_rect(size)));
    }
