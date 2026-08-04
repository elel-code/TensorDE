
    #[test]
    fn blank_pane_click_clears_multi_selection() {
        let mut scene = test_scene(
            vec![
                test_entry("alpha.txt", false),
                test_entry("bravo.txt", false),
            ],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(520, 360);
        scene.panes[ShellPaneId::SLOT_0]
            .selection
            .select_indexes(&[0, 1]);
        let content = scene
            .pane_geometry(ShellPaneId::SLOT_0, size)
            .unwrap()
            .content;
        let blank = ViewPoint {
            x: content.right() - 2.0,
            y: content.bottom() - 2.0,
        };

        assert!(scene.begin_pane_pointer(
            SelectionClick {
                point: blank,
                extend: false,
                toggle: false,
            },
            size,
        ));
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].selection.len(), 0);
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].selection.anchor, None);
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].selection.focus, None);
        let _ = scene.end_pane_pointer(blank, size);
        assert!(scene.rubber_band.is_none());
    }

    #[test]
    fn context_target_selects_unselected_item_from_retained_hit_test() {
        let mut scene = test_scene(
            vec![
                test_entry("alpha.txt", false),
                test_entry("bravo.txt", false),
                test_entry("charlie.txt", true),
            ],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(420, 340);
        let item = scene
            .layout(size)
            .item(1)
            .expect("second item should layout");
        let point = ViewPoint {
            x: scene.content_origin_x(size) + item.visual_rect.x + 2.0,
            y: item.visual_rect.y + scene.content_origin_y() + 2.0,
        };

        assert!(scene.open_context_target(point, size));
        assert_eq!(
            scene.panes[ShellPaneId::SLOT_0]
                .selection
                .selected
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![1]
        );
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].selection.focus, Some(1));
        assert_eq!(
            scene.hovered_item,
            Some(ShellPaneItemTarget {
                pane: ShellPaneId::SLOT_0,
                index: 1,
            })
        );
        assert_eq!(scene.selection_changes, 1);
        assert_eq!(scene.context_target_changes, 1);
        assert_eq!(
            scene.context_target,
            Some(ShellContextTarget::Item {
                pane: ShellPaneId::SLOT_0,
                index: 1,
                path: PathBuf::from("/tmp/bravo.txt"),
                is_dir: false,
                selection_count: 1,
            })
        );
    }

    #[test]
    fn context_target_preserves_multi_selection_for_selected_item() {
        let mut scene = test_scene(
            vec![
                test_entry("alpha.txt", false),
                test_entry("bravo.txt", false),
                test_entry("charlie.txt", false),
            ],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(420, 260);
        assert!(
            scene.panes[ShellPaneId::SLOT_0]
                .selection
                .select_indexes(&[0, 2])
        );
        let item = scene
            .layout(size)
            .item(0)
            .expect("first item should layout");
        let point = ViewPoint {
            x: scene.content_origin_x(size) + item.visual_rect.x + 2.0,
            y: item.visual_rect.y + scene.content_origin_y() + 2.0,
        };

        assert!(scene.open_context_target(point, size));
        assert_eq!(
            scene.panes[ShellPaneId::SLOT_0]
                .selection
                .selected
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![0, 2]
        );
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].selection.focus, Some(0));
        assert_eq!(
            scene.context_target,
            Some(ShellContextTarget::Item {
                pane: ShellPaneId::SLOT_0,
                index: 0,
                path: PathBuf::from("/tmp/alpha.txt"),
                is_dir: false,
                selection_count: 2,
            })
        );
    }

    #[test]
    fn context_target_uses_blank_content_without_rubber_band() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(420, 260);
        assert!(
            scene.panes[ShellPaneId::SLOT_0]
                .selection
                .apply_navigation(0, false)
        );
        scene.rubber_band = Some(RubberBand {
            start: ViewPoint { x: 0.0, y: 0.0 },
            current: ViewPoint { x: 12.0, y: 12.0 },
            active: true,
            mode: RubberBandMode::Replace,
            base_selection: scene.panes[ShellPaneId::SLOT_0].selection.clone(),
        });
        let content = scene
            .pane_geometry(ShellPaneId::SLOT_0, size)
            .unwrap()
            .content;
        let point = ViewPoint {
            x: content.right() - 4.0,
            y: scene.content_origin_y() + 4.0,
        };

        assert!(scene.open_context_target(point, size));
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].selection.len(), 0);
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].selection.anchor, None);
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].selection.focus, None);
        assert_eq!(scene.hovered_item, None);
        assert!(scene.rubber_band.is_none());
        assert_eq!(
            scene.context_target,
            Some(ShellContextTarget::Blank {
                pane: ShellPaneId::SLOT_0,
                path: PathBuf::from("/tmp"),
            })
        );

        let status_point = ViewPoint {
            x: scene.status_bar_rect(size).x + 10.0,
            y: scene.status_bar_rect(size).y + 2.0,
        };
        assert!(scene.open_context_target(status_point, size));
        assert_eq!(scene.context_target, None);
    }

    #[test]
    fn context_target_blank_clears_multi_selection() {
        let mut scene = test_scene(
            vec![
                test_entry("alpha.txt", false),
                test_entry("bravo.txt", false),
            ],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(420, 260);
        assert!(
            scene.panes[ShellPaneId::SLOT_0]
                .selection
                .select_indexes(&[0, 1])
        );
        let content = scene
            .pane_geometry(ShellPaneId::SLOT_0, size)
            .unwrap()
            .content;
        let point = ViewPoint {
            x: content.right() - 4.0,
            y: scene.content_origin_y() + 4.0,
        };

        assert!(scene.open_context_target(point, size));
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].selection.len(), 0);
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].selection.anchor, None);
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].selection.focus, None);
        assert_eq!(scene.selection_changes, 1);
        assert_eq!(
            scene.context_target,
            Some(ShellContextTarget::Blank {
                pane: ShellPaneId::SLOT_0,
                path: PathBuf::from("/tmp"),
            })
        );
    }

    #[test]
    fn details_context_target_unselected_row_blank_side_clears_multi_selection() {
        let mut scene = test_scene(
            vec![
                test_entry("alpha.txt", false),
                test_entry("bravo.txt", false),
                test_entry("charlie.txt", false),
            ],
            ShellViewMode::Details,
        );
        let size = PhysicalSize::new(900, 320);
        assert!(
            scene.panes[ShellPaneId::SLOT_0]
                .selection
                .select_indexes(&[0, 1])
        );
        let projection = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();
        let content = projection.geometry.content;
        let row = projection.visible_items[2].layout;
        let blank_side = ViewPoint {
            x: content.right() - 4.0,
            y: content.y + row.item_rect.y + row.item_rect.height / 2.0,
        };

        assert_eq!(scene.hit_test_screen_point(blank_side, size), Some(2));
        assert!(scene.open_context_target(blank_side, size));
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].selection.len(), 0);
        assert_eq!(scene.selection_changes, 1);
        assert_eq!(
            scene.context_target,
            Some(ShellContextTarget::Blank {
                pane: ShellPaneId::SLOT_0,
                path: PathBuf::from("/tmp"),
            })
        );
    }

    #[test]
    fn context_menu_opens_item_actions_and_records_action_hits() {
        let mut scene = test_scene(vec![test_entry("folder", true)], ShellViewMode::Icons);
        let size = PhysicalSize::new(420, 260);
        let item = scene.layout(size).item(0).expect("item should layout");
        let point = ViewPoint {
            x: scene.content_origin_x(size) + item.visual_rect.x + 2.0,
            y: item.visual_rect.y + scene.content_origin_y() + 2.0,
        };

        assert!(scene.open_context_menu(point, size));
        let menu = scene
            .context_menu
            .as_ref()
            .expect("context menu should open");
        assert!(matches!(
            menu.target,
            ShellContextTarget::Item {
                pane: ShellPaneId::SLOT_0,
                index: 0,
                is_dir: true,
                ..
            }
        ));
        assert_eq!(
            context_menu_actions(&menu.target).first().copied(),
            Some(ShellContextMenuAction::OpenInNewPane)
        );

        let rect = context_menu_rect(menu, size);
        let first_row = ViewPoint {
            x: rect.x + 8.0,
            y: rect.y + 8.0,
        };
        assert_eq!(
            scene.context_menu_action_at_screen_point(first_row, size),
            Some(ShellContextMenuAction::OpenInNewPane)
        );
        assert!(scene.set_pointer(first_row, size));
        assert_eq!(
            scene
                .context_menu
                .as_ref()
                .and_then(|menu| menu.hovered_row),
            Some(0)
        );
        assert_eq!(
            scene.activate_or_close_context_menu(first_row, size),
            Some(ShellContextMenuAction::OpenInNewPane)
        );
        assert!(scene.context_menu.is_none());
        assert_eq!(scene.context_menu_actions, 1);
    }

    #[test]
    fn context_menu_clamps_blank_actions_inside_window() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        let size = PhysicalSize::new(240, 180);
        let content = scene
            .pane_geometry(ShellPaneId::SLOT_0, size)
            .unwrap()
            .content;
        let point = ViewPoint {
            x: content.right() - 2.0,
            y: content.bottom() - 2.0,
        };

        assert!(scene.open_context_menu(point, size));
        let menu = scene
            .context_menu
            .as_ref()
            .expect("blank context menu should open");
        assert!(matches!(menu.target, ShellContextTarget::Blank { .. }));
        assert_eq!(
            context_menu_actions(&menu.target).first().copied(),
            Some(ShellContextMenuAction::CreateNew)
        );
        let rect = context_menu_rect(menu, size);
        assert!(rect.x >= CONTEXT_MENU_VIEWPORT_MARGIN);
        assert!(rect.y >= CONTEXT_MENU_VIEWPORT_MARGIN);
        assert!(rect.right() <= size.width as f32 - CONTEXT_MENU_VIEWPORT_MARGIN + f32::EPSILON);
        assert!(rect.bottom() <= size.height as f32 - CONTEXT_MENU_VIEWPORT_MARGIN + f32::EPSILON);

        assert_eq!(
            scene.activate_or_close_context_menu(
                ViewPoint {
                    x: CONTEXT_MENU_VIEWPORT_MARGIN,
                    y: CONTEXT_MENU_VIEWPORT_MARGIN,
                },
                size,
            ),
            None
        );
        assert!(scene.context_menu.is_none());
        assert_eq!(scene.context_menu_actions, 0);
    }

    #[test]
    fn context_menu_uses_original_metrics_and_flips_near_edges() {
        let target = ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_0,
            path: PathBuf::from("/tmp"),
        };
        let menu = ShellContextMenu::new(target, ViewPoint { x: 390.0, y: 280.0 });
        let size = PhysicalSize::new(420, 420);
        let rect = context_menu_rect(&menu, size);

        assert_eq!(rect.width, 260.0);
        assert_eq!(
            rect.height,
            CONTEXT_MENU_VERTICAL_PADDING * 2.0
                + context_menu_actions(&menu.target).len() as f32 * CONTEXT_MENU_ROW_HEIGHT
        );
        assert!(rect.x < menu.position.x);
        assert!(rect.y < menu.position.y);
        assert!(rect.right() <= size.width as f32 - CONTEXT_MENU_VIEWPORT_MARGIN + f32::EPSILON);
        assert!(rect.bottom() <= size.height as f32 - CONTEXT_MENU_VIEWPORT_MARGIN + f32::EPSILON);
    }

    #[test]
    fn context_menu_hit_testing_respects_vertical_padding() {
        let target = ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_0,
            path: PathBuf::from("/tmp"),
        };
        let menu = ShellContextMenu::new(target, ViewPoint { x: 40.0, y: 40.0 });
        let size = PhysicalSize::new(420, 420);
        let rect = context_menu_rect(&menu, size);

        assert_eq!(
            context_menu_row_at_screen_point(
                &menu,
                ViewPoint {
                    x: rect.x + 12.0,
                    y: rect.y + 2.0
                },
                size,
                1.0,
            ),
            None
        );
        assert_eq!(
            context_menu_row_at_screen_point(
                &menu,
                ViewPoint {
                    x: rect.x + 12.0,
                    y: rect.y + CONTEXT_MENU_VERTICAL_PADDING + 2.0
                },
                size,
                1.0,
            ),
            Some(0)
        );
        assert_eq!(
            context_menu_row_at_screen_point(
                &menu,
                ViewPoint {
                    x: rect.x + 12.0,
                    y: rect.bottom() - 2.0
                },
                size,
                1.0,
            ),
            None
        );
    }

    #[test]
    fn context_menu_separator_rows_match_original_grouping() {
        let blank = ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_0,
            path: PathBuf::from("/tmp"),
        };
        let blank_actions = context_menu_actions(&blank);
        let paste_row = blank_actions
            .iter()
            .position(|action| *action == ShellContextMenuAction::Paste)
            .unwrap();
        let select_all_row = blank_actions
            .iter()
            .position(|action| *action == ShellContextMenuAction::SelectAll)
            .unwrap();
        let toggle_hidden_row = blank_actions
            .iter()
            .position(|action| *action == ShellContextMenuAction::ToggleHiddenFiles)
            .unwrap();
        let view_mode_row = blank_actions
            .iter()
            .position(|action| *action == ShellContextMenuAction::ViewMode)
            .unwrap();
        let split_row = blank_actions
            .iter()
            .position(|action| *action == ShellContextMenuAction::SplitPane)
            .unwrap();
        let properties_row = blank_actions
            .iter()
            .position(|action| *action == ShellContextMenuAction::Properties)
            .unwrap();

        assert!(!context_menu_separator_before(&blank, 0));
        assert!(context_menu_separator_before(&blank, paste_row));
        assert!(context_menu_separator_before(&blank, select_all_row));
        assert!(context_menu_separator_before(&blank, view_mode_row));
        assert!(!context_menu_separator_before(&blank, toggle_hidden_row));
        assert!(!context_menu_separator_before(&blank, split_row));
        assert!(context_menu_separator_before(&blank, properties_row));

        let item = ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: PathBuf::from("/tmp/file.txt"),
            is_dir: false,
            selection_count: 1,
        };
        let item_actions = context_menu_actions(&item);
        let copy_row = item_actions
            .iter()
            .position(|action| *action == ShellContextMenuAction::Copy)
            .unwrap();
        let rename_row = item_actions
            .iter()
            .position(|action| *action == ShellContextMenuAction::Rename)
            .unwrap();
        let properties_row = item_actions
            .iter()
            .position(|action| *action == ShellContextMenuAction::Properties)
            .unwrap();

        assert!(!context_menu_separator_before(&item, 0));
        assert!(context_menu_separator_before(&item, copy_row));
        assert!(context_menu_separator_before(&item, rename_row));
        assert!(context_menu_separator_before(&item, properties_row));
    }
