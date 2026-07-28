
    #[test]
    fn render_damage_bounds_location_draft_transition() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        scene.panes[ShellPaneId::SLOT_0].path = PathBuf::from("/tmp");
        let size = PhysicalSize::new(700, 320);
        assert!(scene.apply_location_command(LocationCommand::Activate, size));
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

        assert!(scene.apply_location_command(LocationCommand::Insert("/alpha".to_string()), size));
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

        let damage = ShellRenderDamage::between(Some(&initial), &changed, false);

        assert_eq!(damage.kind, ShellRenderDamageKind::Bounded);
        assert_eq!(damage.rect_count, 1);
        assert_eq!(damage.bounds, changed.location_draft_rect);
        assert!(damage.area_px < rect_area(full_surface_rect(size)));
    }

    #[test]
    fn render_damage_is_clean_for_detached_properties_content_transition() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        scene.properties_overlay = Some(ShellPropertiesOverlay {
            title: "alpha.txt".to_string(),
            rows: vec![property_row("Size", "10 B".to_string())],
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

        let overlay = scene.properties_overlay.as_mut().unwrap();
        overlay.rows[0].value = "12 B".to_string();
        scene.properties_changes += 1;
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

        let damage = ShellRenderDamage::between(Some(&initial), &changed, false);

        assert_eq!(damage.kind, ShellRenderDamageKind::Clean);
        assert_eq!(damage.rect_count, 0);
        assert_eq!(damage.bounds, None);
    }

    #[test]
    fn render_damage_bounds_task_status_content_transition() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(900, 420);
        scene.task_statuses.record(ShellTaskStatus::running(
            1,
            "Copying",
            "1 of 3 items",
            false,
        ));
        scene.task_detail_dialog = Some(ShellTaskDetailDialog);
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

        assert!(
            scene
                .task_statuses
                .update_running_detail(1, "2 of 3 items".to_string())
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

        let damage = ShellRenderDamage::between(Some(&initial), &changed, false);
        let task_area = changed.task_area_rect.unwrap();

        assert_eq!(damage.kind, ShellRenderDamageKind::Bounded);
        assert_eq!(damage.rect_count, 1);
        assert_eq!(damage.bounds, Some(task_area));
        assert!(rect_area(task_area) < rect_area(full_surface_rect(size)));
    }

    #[test]
    fn render_damage_is_clean_for_detached_create_dialog_content_transition() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        scene.create_dialog = Some(ShellCreateDialog::new(
            ShellPaneId::SLOT_0,
            PathBuf::from("/tmp"),
            CreateEntryKind::Folder,
            false,
        ));
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

        let dialog = scene.create_dialog.as_mut().unwrap();
        dialog.name = "custom".to_string();
        dialog.replace_on_insert = false;
        scene.create_changes += 1;
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

        let damage = ShellRenderDamage::between(Some(&initial), &changed, false);

        assert_eq!(damage.kind, ShellRenderDamageKind::Clean);
    }

    #[test]
    fn render_damage_is_clean_for_detached_rename_dialog_content_transition() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        scene.rename_dialog = ShellRenameDialog::new(
            ShellPaneId::SLOT_0,
            PathBuf::from("/tmp/alpha.txt"),
            false,
            false,
        );
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

        let dialog = scene.rename_dialog.as_mut().unwrap();
        dialog.name = "beta.txt".to_string();
        dialog.replace_on_insert = false;
        scene.rename_changes += 1;
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

        let damage = ShellRenderDamage::between(Some(&initial), &changed, false);

        assert_eq!(damage.kind, ShellRenderDamageKind::Clean);
    }

    #[test]
    fn render_damage_is_clean_for_detached_open_with_chooser_content_transition() {
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

        assert!(scene.apply_open_with_command(OpenWithCommand::Insert("paint".to_string())));
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

        let damage = ShellRenderDamage::between(Some(&initial), &changed, false);

        assert_eq!(damage.kind, ShellRenderDamageKind::Clean);
    }

    #[test]
    fn render_damage_falls_back_to_full_when_rubber_band_changes_selection() {
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

        scene.rubber_band = Some(RubberBand {
            start: ViewPoint { x: 10.0, y: 12.0 },
            current: ViewPoint { x: 110.0, y: 118.0 },
            active: true,
            mode: RubberBandMode::Replace,
            base_selection: scene.panes[ShellPaneId::SLOT_0].selection.clone(),
        });
        assert!(
            scene.panes[ShellPaneId::SLOT_0]
                .selection
                .select_indexes(&[0])
        );
        scene.selection_changes += 1;
        scene.rubber_band_updates += 1;
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let selected = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        let damage = ShellRenderDamage::between(Some(&initial), &selected, false);

        assert_eq!(damage.kind, ShellRenderDamageKind::Full);
        assert_eq!(damage.bounds, Some(full_surface_rect(size)));
    }

    #[test]
    fn render_damage_falls_back_to_full_for_non_hover_dirty_state() {
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

        scene.panes[ShellPaneId::SLOT_0].scroll_y = 24.0;
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

        assert_eq!(damage.kind, ShellRenderDamageKind::Full);
        assert_eq!(damage.bounds, Some(full_surface_rect(size)));
    }

    #[test]
    fn render_damage_bounds_context_menu_hover_transition() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        scene.context_menu = Some(ShellContextMenu::new(
            ShellContextTarget::Blank {
                pane: ShellPaneId::SLOT_0,
                path: PathBuf::from("/tmp"),
            },
            ViewPoint { x: 20.0, y: 30.0 },
        ));
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

        scene.context_menu.as_mut().unwrap().hovered_row = Some(0);
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
    fn render_damage_bounds_context_menu_open_and_close() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let closed = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        scene.context_menu = Some(ShellContextMenu::new(
            ShellContextTarget::Blank {
                pane: ShellPaneId::SLOT_0,
                path: PathBuf::from("/tmp"),
            },
            ViewPoint { x: 20.0, y: 30.0 },
        ));
        let projections = ShellPaneId::ALL
            .into_iter()
            .filter_map(|kind| scene.pane_projection(kind, size))
            .collect::<Vec<_>>();
        let opened = ShellRenderDamageSnapshot::from_scene(
            &scene,
            size,
            &projections,
            ShellRenderDirtyKey::from_scene(&scene, size),
        );

        let open_damage = ShellRenderDamage::between(Some(&closed), &opened, false);
        let close_damage = ShellRenderDamage::between(Some(&opened), &closed, false);

        assert_eq!(open_damage.kind, ShellRenderDamageKind::Bounded);
        assert_eq!(open_damage.rect_count, 1);
        assert_eq!(
            open_damage.bounds,
            opened.context_menu.as_ref().map(|state| state.overlay_rect)
        );
        assert!(open_damage.area_px < rect_area(full_surface_rect(size)));
        assert_eq!(close_damage.kind, ShellRenderDamageKind::Bounded);
        assert_eq!(
            close_damage.bounds,
            opened.context_menu.as_ref().map(|state| state.overlay_rect)
        );
    }
