
    #[test]
    fn task_area_opens_detail_dialog_and_clear_keeps_running_tasks_visible() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(760, 520);
        assert_eq!(
            scene.open_task_detail_dialog_at_screen_point(ViewPoint { x: 1.0, y: 1.0 }, size),
            None
        );

        scene.record_task_status(ShellTaskStatus::completed("Copied", "alpha.txt", false));
        scene.record_task_status(ShellTaskStatus::running(
            7,
            "Copying",
            "1 item to /tmp",
            false,
        ));
        let task_area = scene.places_task_area_rect(size).unwrap();
        assert_eq!(
            scene.open_task_detail_dialog_at_screen_point(
                ViewPoint {
                    x: task_area.x + 4.0,
                    y: task_area.y + 4.0,
                },
                size,
            ),
            Some(true)
        );
        assert!(scene.is_task_detail_dialog_open());

        let rect = task_detail_dialog_rect(scene.task_statuses.len(), size);
        assert_eq!(
            scene.task_detail_dialog_click_at_screen_point(
                ViewPoint {
                    x: task_detail_cancel_button_rect(rect).x + 2.0,
                    y: task_detail_cancel_button_rect(rect).y + 2.0,
                },
                size,
            ),
            TaskDetailDialogClick::Cancel
        );
        assert_eq!(
            scene.task_detail_dialog_click_at_screen_point(
                ViewPoint {
                    x: task_detail_clear_button_rect(rect).x + 2.0,
                    y: task_detail_clear_button_rect(rect).y + 2.0,
                },
                size,
            ),
            TaskDetailDialogClick::Clear
        );
        assert!(scene.clear_task_statuses());
        assert_eq!(scene.task_statuses.len(), 1);
        assert_eq!(scene.task_statuses[0].kind, ShellTaskStatusKind::Running);
        assert!(scene.is_task_detail_dialog_open());
    }

    #[test]
    fn task_detail_cancel_marks_running_task_and_returns_task_id() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(760, 520);
        scene.record_task_status(ShellTaskStatus::running(
            42,
            "Copying",
            "1 item to /tmp",
            false,
        ));
        let task_area = scene.places_task_area_rect(size).unwrap();
        assert_eq!(
            scene.open_task_detail_dialog_at_screen_point(
                ViewPoint {
                    x: task_area.x + 4.0,
                    y: task_area.y + 4.0,
                },
                size,
            ),
            Some(true)
        );

        let rect = task_detail_dialog_rect(scene.task_statuses.len(), size);
        assert_eq!(
            scene.task_detail_dialog_click_at_screen_point(
                ViewPoint {
                    x: task_detail_dismiss_button_rect(rect, 0).x + 2.0,
                    y: task_detail_dismiss_button_rect(rect, 0).y + 2.0,
                },
                size,
            ),
            TaskDetailDialogClick::Dismiss(0)
        );
        let (changed, task_id) = scene.dismiss_task_status(0);
        assert!(changed);
        assert_eq!(task_id, Some(42));
        assert_eq!(scene.task_statuses[0].kind, ShellTaskStatusKind::Cancelled);
    }

    #[test]
    fn keyboard_navigation_uses_icons_columns_and_page_stride() {
        let size = PhysicalSize::new(360, 240);
        let layout = IconsLayout::new(
            20,
            IconsLayoutOptions {
                viewport_width: size.width as f32,
                viewport_height: content_height(size),
                reserved_bottom: 0.0,
                scroll_x: 0.0,
                scroll_y: 0.0,
                padding: 8.0,
                gap: 12.0,
                item_width: ICONS_ITEM_WIDTH,
                item_height: 106.0,
                icon_size: ICONS_ICON_SIZE,
                text_height: 18.0,
            },
        );
        let layout = ShellLayout::Icons(layout);

        assert_eq!(
            navigation_target(NavigationAction::Right, 0, 20, &layout),
            Some(1)
        );
        assert_eq!(
            navigation_target(NavigationAction::Down, 0, 20, &layout),
            Some(match &layout {
                ShellLayout::Icons(layout) => layout.columns_per_row(),
                _ => unreachable!(),
            })
        );
        assert_eq!(
            navigation_target(NavigationAction::Up, 1, 20, &layout),
            Some(0)
        );
        assert_eq!(
            navigation_target(NavigationAction::End, 0, 20, &layout),
            Some(19)
        );
        assert_eq!(
            navigation_target(NavigationAction::PageDown, 0, 20, &layout),
            Some(layout.visible_items().len())
        );
    }

    #[test]
    fn compact_navigation_uses_column_major_rows() {
        let size = PhysicalSize::new(320, 180);
        let compact = CompactLayout::new(
            20,
            CompactLayoutOptions {
                viewport_width: size.width as f32,
                viewport_height: content_height(size),
                reserved_bottom: 0.0,
                scroll_x: 0.0,
                scroll_y: 0.0,
                padding: 6.0,
                side_padding: 8.0,
                gap: 8.0,
                text_gap: 8.0,
                item_width: 236.0,
                item_height: 44.0,
                icon_size: COMPACT_ICON_SIZE,
                text_height: 18.0,
            },
        );
        let layout = ShellLayout::Compact(ShellCompactLayout::new(compact, vec![0.0; 20]));
        let rows = match &layout {
            ShellLayout::Compact(layout) => layout.rows_per_column(),
            _ => unreachable!(),
        };

        assert_eq!(
            navigation_target(NavigationAction::Down, 0, 20, &layout),
            Some(1)
        );
        assert_eq!(
            navigation_target(NavigationAction::Right, 0, 20, &layout),
            Some(rows)
        );
        assert_eq!(
            navigation_target(NavigationAction::Left, rows, 20, &layout),
            Some(0)
        );
    }

    #[test]
    fn compact_layout_uses_longest_name_per_column_and_per_item_visual_width() {
        let scene = test_scene(
            vec![
                test_entry("a", false),
                test_entry("very-wide-filename.txt", false),
                test_entry("b", false),
                test_entry("c", false),
            ],
            ShellViewMode::Compact,
        );
        let size = PhysicalSize::new(700, 250);
        let layout = match scene.layout(size) {
            ShellLayout::Compact(layout) => layout,
            _ => unreachable!(),
        };

        assert_eq!(layout.rows_per_column(), 3);
        let short_first_column = layout.item(0).unwrap();
        let long_first_column = layout.item(1).unwrap();
        let short_second_column = layout.item(3).unwrap();

        assert_eq!(
            short_first_column.item_rect.width,
            long_first_column.item_rect.width
        );
        assert!(long_first_column.item_rect.width > short_second_column.item_rect.width);
        assert!(short_first_column.visual_rect.width < long_first_column.visual_rect.width);
        assert!(short_first_column.visual_rect.width < short_first_column.item_rect.width);
    }

    #[test]
    fn compact_layout_keeps_the_full_shaped_filename_inside_its_text_rect() {
        let name = "README.zh-CN.md";
        let scene = test_scene(vec![test_entry(name, false)], ShellViewMode::Compact);
        let size = PhysicalSize::new(700, 250);
        let options = scene.compact_options(size);
        let shaped_width = scene.text_hit_tests.borrow_mut().no_wrap_width(
            name,
            TEXT_FONT_SIZE * scene.text_line_height() / TEXT_LINE_HEIGHT,
            scene.text_line_height(),
        );
        let item = match scene.layout(size) {
            ShellLayout::Compact(layout) => layout.item(0).expect("compact item"),
            _ => unreachable!(),
        };

        assert!(
            item.text_rect.width.ceil()
                >= shaped_width.ceil() + options.padding.mul_add(2.0, 0.0),
            "text rect {} did not contain shaped width {} plus padding {}",
            item.text_rect.width,
            shaped_width,
            options.padding * 2.0,
        );
    }

    #[test]
    fn details_navigation_and_header_hit_test_are_row_based() {
        let scene = test_scene(
            vec![
                test_entry("alpha.txt", false),
                test_entry("bravo.txt", false),
                test_entry("charlie.txt", false),
            ],
            ShellViewMode::Details,
        );
        let size = PhysicalSize::new(420, 220);
        let header_point = ViewPoint {
            x: scene.content_origin_x(size) + 12.0,
            y: scene.details_header_y() + 4.0,
        };
        assert_eq!(scene.hit_test_screen_point(header_point, size), None);

        let row_point = ViewPoint {
            x: scene.content_origin_x(size) + 12.0,
            y: scene.content_origin_y() + 4.0,
        };
        assert_eq!(scene.hit_test_screen_point(row_point, size), Some(0));

        let layout = scene.layout(size);
        assert_eq!(
            navigation_target(NavigationAction::Down, 0, 3, &layout),
            Some(1)
        );
        assert_eq!(
            navigation_target(NavigationAction::PageDown, 0, 3, &layout),
            Some(2)
        );
    }

    #[test]
    fn keyboard_navigation_updates_focus_and_shift_range() {
        let mut selection = ShellSelection::default();

        assert!(selection.apply_navigation(3, false));
        assert_eq!(selection.anchor, Some(3));
        assert_eq!(selection.focus, Some(3));
        assert_eq!(
            selection.selected.iter().copied().collect::<Vec<_>>(),
            vec![3]
        );

        assert!(selection.apply_navigation(7, true));
        assert_eq!(selection.anchor, Some(3));
        assert_eq!(selection.focus, Some(7));
        assert_eq!(
            selection.selected.iter().copied().collect::<Vec<_>>(),
            vec![3, 4, 5, 6, 7]
        );

        assert!(selection.apply_navigation(5, true));
        assert_eq!(selection.anchor, Some(3));
        assert_eq!(selection.focus, Some(5));
        assert_eq!(
            selection.selected.iter().copied().collect::<Vec<_>>(),
            vec![3, 4, 5]
        );
    }

    #[test]
    fn rubber_band_selection_supports_replace_extend_and_toggle() {
        let mut base = ShellSelection::default();
        assert!(base.apply_navigation(2, false));

        let mut selection = base.clone();
        assert!(selection.apply_rubber_band(&base, &[4, 5], RubberBandMode::Replace));
        assert_eq!(
            selection.selected.iter().copied().collect::<Vec<_>>(),
            vec![4, 5]
        );
        assert_eq!(selection.anchor, Some(4));
        assert_eq!(selection.focus, Some(5));

        let mut selection = base.clone();
        assert!(selection.apply_rubber_band(&base, &[4, 5], RubberBandMode::Extend));
        assert_eq!(
            selection.selected.iter().copied().collect::<Vec<_>>(),
            vec![2, 4, 5]
        );
        assert_eq!(selection.anchor, Some(2));
        assert_eq!(selection.focus, Some(5));

        let mut selection = base.clone();
        assert!(selection.apply_rubber_band(&base, &[2, 3], RubberBandMode::Toggle));
        assert_eq!(
            selection.selected.iter().copied().collect::<Vec<_>>(),
            vec![3]
        );
        assert_eq!(selection.focus, Some(3));
    }

    #[test]
    fn rubber_band_drag_from_blank_space_selects_intersecting_visual_rects() {
        let mut scene = test_scene(
            vec![
                test_entry("alpha.txt", false),
                test_entry("bravo.txt", false),
                test_entry("charlie.txt", false),
            ],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(420, 260);
        let layout = scene.layout(size);
        let item = layout.item(0).expect("test item should layout");
        let start = ViewPoint {
            x: scene.content_origin_x(size) + scene.content_width(size) - 2.0,
            y: scene.content_origin_y() + 1.0,
        };
        let current = ViewPoint {
            x: scene.content_origin_x(size) + item.visual_rect.right() - 1.0,
            y: item.visual_rect.bottom() - scene.panes[ShellPaneId::SLOT_0].scroll_y
                + scene.content_origin_y()
                - 1.0,
        };

        assert!(!scene.begin_pane_pointer(
            SelectionClick {
                point: start,
                extend: false,
                toggle: false,
            },
            size,
        ));
        assert!(scene.set_pointer(current, size));
        assert!(scene.panes[ShellPaneId::SLOT_0].selection.contains(0));
        assert!(scene.rubber_band.as_ref().is_some_and(|band| band.active));
        assert!(scene.end_pane_pointer(current, size));
        assert!(scene.rubber_band.is_none());
    }

    #[test]
    fn clamped_screen_to_content_point_stays_inside_content_viewport() {
        let content_rect = ViewRect {
            x: 0.0,
            y: TOP_BAR_HEIGHT,
            width: 320.0,
            height: 160.0,
        };
        assert_eq!(
            clamped_screen_to_content_point(
                ViewPoint {
                    x: -10.0,
                    y: TOP_BAR_HEIGHT - 20.0,
                },
                ViewPoint { x: 0.0, y: 40.0 },
                content_rect,
            ),
            ViewPoint { x: 0.0, y: 40.0 }
        );
        assert_eq!(
            clamped_screen_to_content_point(
                ViewPoint { x: 500.0, y: 500.0 },
                ViewPoint { x: 0.0, y: 40.0 },
                content_rect,
            ),
            ViewPoint { x: 320.0, y: 200.0 }
        );
    }
