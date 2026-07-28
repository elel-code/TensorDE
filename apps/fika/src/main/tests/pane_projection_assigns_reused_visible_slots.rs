    #[test]
    fn pane_projection_assigns_reused_visible_slots() {
        let mut scene = test_scene(
            (0..80)
                .map(|index| test_entry(&format!("entry-{index:02}.txt"), false))
                .collect(),
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(420, 260);

        let initial_stats = scene.update_visible_slot_pools(size);
        assert!(initial_stats.active > 0);
        assert_eq!(initial_stats.allocated, initial_stats.active);
        let initial_projection = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();
        let retained_slot = initial_projection.visible_items[0].slot_id;
        assert_ne!(retained_slot, 0);
        assert!(
            initial_projection
                .visible_items
                .iter()
                .all(|item| item.slot_id != 0)
        );

        let next_stats = scene.update_visible_slot_pools(size);
        assert_eq!(next_stats.reused, next_stats.active);
        let next_projection = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();
        assert_eq!(next_projection.visible_items[0].slot_id, retained_slot);
    }

    #[test]
    fn prepared_pane_projections_match_direct_projection() {
        let mut scene = test_scene(
            (0..60)
                .map(|index| test_entry(&format!("entry-{index:02}.txt"), index % 4 == 0))
                .collect(),
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(700, 320);

        let mut layouts = scene.prepare_frame_projection_layouts(size);
        scene.update_visible_slot_pools_for_projection_layouts(&mut layouts);
        assert!(
            layouts
                .layouts
                .iter()
                .flat_map(|projection| &projection.visible_items)
                .all(|item| item.path.is_none())
        );
        let frame_projections = scene.pane_projections_from_layouts(layouts);
        let prepared = frame_projections
            .projections()
            .iter()
            .find(|projection| projection.geometry.kind == ShellPaneId::SLOT_0)
            .unwrap();
        let direct = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();

        assert_eq!(prepared.geometry, direct.geometry);
        assert_eq!(prepared.scroll_metrics, direct.scroll_metrics);
        assert_eq!(prepared.visible_items, direct.visible_items);
        assert!(prepared.visible_items.iter().all(|item| item.slot_id != 0));
        assert_eq!(prepared.view.path, direct.view.path);
        assert_eq!(prepared.view.view_mode, direct.view.view_mode);
    }

    #[test]
    fn render_dirty_key_with_projections_matches_scene_lookup() {
        let scene = test_scene(
            (0..80)
                .map(|index| test_entry(&format!("entry-{index:02}.txt"), index % 7 == 0))
                .collect(),
            ShellViewMode::Details,
        );
        let size = PhysicalSize::new(700, 320);
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let context = ShellRenderDirtyKeyContext::from_scene(&scene, &projections);

        assert_eq!(
            ShellRenderDirtyKey::from_scene(&scene, size),
            ShellRenderDirtyKey::from_scene_with_context(&scene, size, &context)
        );
        assert_eq!(
            ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size),
            ShellRenderDirtyKey::from_scene_ignoring_hover_with_context(&scene, size, &context)
        );
        assert_eq!(
            ShellRenderDirtyKey::from_scene_ignoring_folder_preview_roles(&scene, size),
            ShellRenderDirtyKey::from_scene_ignoring_folder_preview_roles_with_context(
                &scene, size, &context
            )
        );
        assert_eq!(
            ShellRenderDirtyKey::from_scene_ignoring_hover_and_folder_preview_roles(&scene, size),
            ShellRenderDirtyKey::from_scene_ignoring_hover_and_folder_preview_roles_with_context(
                &scene, size, &context
            )
        );
    }

    #[test]
    fn render_dirty_key_tracks_scroll_and_hover_state() {
        let mut scene = test_scene(
            vec![test_entry("alpha.txt", false), test_entry("folder", true)],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(700, 320);
        let initial = ShellRenderDirtyKey::from_scene(&scene, size);

        scene.panes[ShellPaneId::SLOT_0].scroll_y = 24.0;
        let scrolled = ShellRenderDirtyKey::from_scene(&scene, size);
        assert_ne!(initial, scrolled);

        scene.hovered_item = Some(ShellPaneItemTarget {
            pane: ShellPaneId::SLOT_0,
            index: 1,
        });
        let hovered = ShellRenderDirtyKey::from_scene(&scene, size);
        assert_ne!(scrolled, hovered);
    }

    #[test]
    fn render_dirty_key_tracks_menu_hover_state() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        scene.context_menu = Some(ShellContextMenu::new(
            ShellContextTarget::Blank {
                pane: ShellPaneId::SLOT_0,
                path: PathBuf::from("/tmp"),
            },
            ViewPoint { x: 20.0, y: 30.0 },
        ));
        let initial = ShellRenderDirtyKey::from_scene(&scene, size);

        scene.context_menu.as_mut().unwrap().hovered_row = Some(1);
        let hovered = ShellRenderDirtyKey::from_scene(&scene, size);

        assert_ne!(initial, hovered);
    }

    #[test]
    fn render_dirty_key_can_ignore_context_menu_hover_for_damage_detection() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        scene.context_menu = Some(ShellContextMenu::new(
            ShellContextTarget::Blank {
                pane: ShellPaneId::SLOT_0,
                path: PathBuf::from("/tmp"),
            },
            ViewPoint { x: 20.0, y: 30.0 },
        ));
        let initial = ShellRenderDirtyKey::from_scene(&scene, size);
        let initial_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        scene.context_menu.as_mut().unwrap().hovered_row = Some(1);
        let hovered = ShellRenderDirtyKey::from_scene(&scene, size);
        let hovered_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        assert_ne!(initial, hovered);
        assert_eq!(initial_hoverless, hovered_hoverless);
    }

    #[test]
    fn render_dirty_key_can_ignore_context_menu_open_for_damage_detection() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let initial = ShellRenderDirtyKey::from_scene(&scene, size);
        let initial_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        scene.context_menu = Some(ShellContextMenu::new(
            ShellContextTarget::Blank {
                pane: ShellPaneId::SLOT_0,
                path: PathBuf::from("/tmp"),
            },
            ViewPoint { x: 20.0, y: 30.0 },
        ));
        let opened = ShellRenderDirtyKey::from_scene(&scene, size);
        let opened_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        assert_ne!(initial, opened);
        assert_eq!(initial_hoverless, opened_hoverless);
    }

    #[test]
    fn render_dirty_key_can_ignore_drop_menu_hover_for_damage_detection() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        scene.drop_menu = Some(ShellDropMenu::new(
            vec![PathBuf::from("/tmp/source.txt")],
            PathBuf::from("/tmp"),
            ShellDropTarget::PaneBlank {
                pane: ShellPaneId::SLOT_0,
                path: PathBuf::from("/tmp"),
            },
            ViewPoint { x: 20.0, y: 30.0 },
        ));
        let initial = ShellRenderDirtyKey::from_scene(&scene, size);
        let initial_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        scene.drop_menu.as_mut().unwrap().hovered_row = Some(1);
        let hovered = ShellRenderDirtyKey::from_scene(&scene, size);
        let hovered_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        assert_ne!(initial, hovered);
        assert_eq!(initial_hoverless, hovered_hoverless);
    }

    #[test]
    fn render_dirty_key_can_ignore_drop_menu_open_for_damage_detection() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let initial = ShellRenderDirtyKey::from_scene(&scene, size);
        let initial_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        scene.drop_menu = Some(ShellDropMenu::new(
            vec![PathBuf::from("/tmp/source.txt")],
            PathBuf::from("/tmp"),
            ShellDropTarget::PaneBlank {
                pane: ShellPaneId::SLOT_0,
                path: PathBuf::from("/tmp"),
            },
            ViewPoint { x: 20.0, y: 30.0 },
        ));
        let opened = ShellRenderDirtyKey::from_scene(&scene, size);
        let opened_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        assert_ne!(initial, opened);
        assert_eq!(initial_hoverless, opened_hoverless);
    }

    #[test]
    fn render_dirty_key_can_ignore_dnd_hover_for_damage_detection() {
        let mut scene = test_scene(vec![test_entry("alpha", true)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let initial = ShellRenderDirtyKey::from_scene(&scene, size);
        let initial_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        scene.dnd_hover_target = Some(ShellDropTarget::PaneItem {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: PathBuf::from("/tmp/alpha"),
            is_dir: true,
        });
        scene.dnd_hover_changes += 1;
        let hovered = ShellRenderDirtyKey::from_scene(&scene, size);
        let hovered_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        assert_ne!(initial, hovered);
        assert_eq!(initial_hoverless, hovered_hoverless);
    }

    #[test]
    fn render_dirty_key_ignores_internal_drag_pointer_motion() {
        // Wayland owns the DnD icon; pointer motion during an active internal
        // drag must not dirty the window surface by itself.
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
        let initial = ShellRenderDirtyKey::from_scene(&scene, size);
        let initial_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        scene.internal_drag.as_mut().unwrap().current = ViewPoint { x: 92.0, y: 96.0 };
        let moved = ShellRenderDirtyKey::from_scene(&scene, size);
        let moved_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        assert_eq!(initial, moved);
        assert_eq!(initial_hoverless, moved_hoverless);
    }

    #[test]
    fn render_dirty_key_ignores_external_drag_lifecycle_without_visible_target() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let initial = ShellRenderDirtyKey::from_scene(&scene, size);

        scene.external_drag = Some(ShellExternalDrag {
            sources: vec![PathBuf::from("/tmp/source.txt")],
            local_source: None,
        });
        let dragging = ShellRenderDirtyKey::from_scene(&scene, size);

        assert_eq!(initial, dragging);
    }

    #[test]
    fn render_dirty_key_can_ignore_places_scroll_for_damage_detection() {
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
        let initial = ShellRenderDirtyKey::from_scene(&scene, size);
        let initial_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        assert!(scene.scroll_places_by(48.0, size));
        let scrolled = ShellRenderDirtyKey::from_scene(&scene, size);
        let scrolled_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        assert_ne!(initial, scrolled);
        assert_eq!(initial_hoverless, scrolled_hoverless);
    }

    #[test]
    fn render_dirty_key_can_ignore_location_draft_for_damage_detection() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        scene.panes[ShellPaneId::SLOT_0].path = PathBuf::from("/tmp");
        let size = PhysicalSize::new(700, 320);
        assert!(scene.apply_location_command(LocationCommand::Activate, size));
        let initial = ShellRenderDirtyKey::from_scene(&scene, size);
        let initial_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        assert!(scene.apply_location_command(LocationCommand::Insert("/alpha".to_string()), size));
        let changed = ShellRenderDirtyKey::from_scene(&scene, size);
        let changed_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        assert_ne!(initial, changed);
        assert_eq!(initial_hoverless, changed_hoverless);
    }

    #[test]
    fn render_dirty_key_ignores_detached_properties_content() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        scene.properties_overlay = Some(ShellPropertiesOverlay {
            title: "alpha.txt".to_string(),
            rows: vec![property_row("Size", "10 B".to_string())],
        });
        let initial = ShellRenderDirtyKey::from_scene(&scene, size);
        let initial_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        let overlay = scene.properties_overlay.as_mut().unwrap();
        overlay.rows[0].value = "12 B".to_string();
        scene.properties_changes += 1;
        let changed = ShellRenderDirtyKey::from_scene(&scene, size);
        let changed_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        assert_eq!(initial, changed);
        assert_eq!(initial_hoverless, changed_hoverless);
    }

    #[test]
    fn render_dirty_key_can_ignore_task_status_content_for_damage_detection() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        scene.task_statuses.record(ShellTaskStatus::running(
            1,
            "Copying",
            "1 of 3 items",
            false,
        ));
        scene.task_detail_dialog = Some(ShellTaskDetailDialog);
        let initial = ShellRenderDirtyKey::from_scene(&scene, size);
        let initial_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        assert!(
            scene
                .task_statuses
                .update_running_detail(1, "2 of 3 items".to_string())
        );
        let changed = ShellRenderDirtyKey::from_scene(&scene, size);
        let changed_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        assert_ne!(initial, changed);
        assert_eq!(initial_hoverless, changed_hoverless);
    }

    #[test]
    fn render_dirty_key_ignores_detached_create_dialog_content() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        scene.create_dialog = Some(ShellCreateDialog::new(
            ShellPaneId::SLOT_0,
            PathBuf::from("/tmp"),
            CreateEntryKind::Folder,
            false,
        ));
        let initial = ShellRenderDirtyKey::from_scene(&scene, size);
        let initial_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        let dialog = scene.create_dialog.as_mut().unwrap();
        dialog.name = "custom".to_string();
        dialog.replace_on_insert = false;
        scene.create_changes += 1;
        let changed = ShellRenderDirtyKey::from_scene(&scene, size);
        let changed_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        assert_eq!(initial, changed);
        assert_eq!(initial_hoverless, changed_hoverless);
    }

    #[test]
    fn render_dirty_key_ignores_detached_rename_dialog_content() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        scene.rename_dialog = ShellRenameDialog::new(
            ShellPaneId::SLOT_0,
            PathBuf::from("/tmp/alpha.txt"),
            false,
            false,
        );
        let initial = ShellRenderDirtyKey::from_scene(&scene, size);
        let initial_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        let dialog = scene.rename_dialog.as_mut().unwrap();
        dialog.name = "beta.txt".to_string();
        dialog.replace_on_insert = false;
        scene.rename_changes += 1;
        let changed = ShellRenderDirtyKey::from_scene(&scene, size);
        let changed_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        assert_eq!(initial, changed);
        assert_eq!(initial_hoverless, changed_hoverless);
    }

    #[test]
    fn render_dirty_key_ignores_detached_open_with_chooser_content() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        scene.open_with_chooser = Some(ShellOpenWithChooser::new(
            PathBuf::from("/tmp/alpha.txt"),
            Some(Arc::from("text/plain")),
            vec![
                MimeApplication {
                    id: "writer.desktop".to_string(),
                    desktop_file: PathBuf::from("/apps/writer.desktop"),
                    name: "Writer".to_string(),
                    exec: "writer %f".to_string(),
                    icon: None,
                    is_default: true,
                },
                MimeApplication {
                    id: "paint.desktop".to_string(),
                    desktop_file: PathBuf::from("/apps/paint.desktop"),
                    name: "Paint".to_string(),
                    exec: "paint %f".to_string(),
                    icon: None,
                    is_default: false,
                },
            ],
            Vec::new(),
        ));
        let initial = ShellRenderDirtyKey::from_scene(&scene, size);
        let initial_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        assert!(scene.apply_open_with_command(OpenWithCommand::Insert("paint".to_string())));
        let changed = ShellRenderDirtyKey::from_scene(&scene, size);
        let changed_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);

        assert_eq!(initial, changed);
        assert_eq!(initial_hoverless, changed_hoverless);
    }
