
    #[test]
    fn rename_dialog_key_input_captures_text_and_commit_controls() {
        let unidentified = PhysicalKey::Unidentified(NativeKeyCode::Unidentified);
        let no_key = Key::Unidentified(NativeKey::Unidentified);

        assert_eq!(
            rename_command_for_key_parts(
                false,
                &unidentified,
                &Key::Character("x".into()),
                &no_key,
            ),
            RenameCommand::Insert("x".to_string())
        );
        assert_eq!(
            rename_command_for_key_parts(
                false,
                &PhysicalKey::Code(KeyCode::Backspace),
                &no_key,
                &no_key,
            ),
            RenameCommand::Backspace
        );
        assert_eq!(
            rename_command_for_key_parts(
                false,
                &unidentified,
                &Key::Named(NamedKey::Enter),
                &no_key,
            ),
            RenameCommand::Commit
        );
        assert_eq!(
            rename_command_for_key_parts(
                false,
                &PhysicalKey::Code(KeyCode::Escape),
                &no_key,
                &no_key,
            ),
            RenameCommand::Cancel
        );
        assert_eq!(
            rename_command_for_key_parts(
                true,
                &PhysicalKey::Code(KeyCode::KeyA),
                &Key::Character("a".into()),
                &Key::Character("a".into()),
            ),
            RenameCommand::Ignore
        );
    }

    #[test]
    fn filter_updates_layout_hit_testing_and_select_all() {
        let mut scene = test_scene(
            vec![
                test_entry("alpha.txt", false),
                test_entry("beta.txt", false),
                test_entry("alphabet.txt", false),
            ],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(420, 340);

        assert!(scene.apply_filter_command(FilterCommand::Activate, size));
        assert!(scene.apply_filter_command(FilterCommand::Insert("alp".to_string()), size));
        assert_eq!(
            scene.panes[ShellPaneId::SLOT_0].filtered_indexes,
            vec![0, 2]
        );
        assert_eq!(scene.filtered_entry_count(), 2);
        assert_eq!(scene.filter_changes, 2);

        let layout = scene.layout(size);
        assert!(layout.item(2).is_none());
        let second = layout.item(1).expect("second filtered item should layout");
        let point = ViewPoint {
            x: scene.content_origin_x(size) + second.visual_rect.x + 2.0,
            y: second.visual_rect.y + scene.content_origin_y() + 2.0,
        };
        assert_eq!(scene.hit_test_screen_point(point, size), Some(2));

        assert!(scene.apply_selection_command(SelectionCommand::SelectAll));
        assert_eq!(
            scene.panes[ShellPaneId::SLOT_0]
                .selection
                .selected
                .iter()
                .copied()
                .collect::<Vec<_>>(),
            vec![0, 2]
        );

        assert!(scene.apply_filter_command(FilterCommand::Backspace, size));
        assert_eq!(scene.filter_pattern, "al");
        assert!(scene.apply_filter_command(FilterCommand::Deactivate, size));
        assert!(!scene.filter_active);
        assert_eq!(scene.filter_pattern, "al");
        assert_eq!(scene.filtered_entry_count(), 2);
        assert!(scene.apply_filter_command(FilterCommand::ClearAndDeactivate, size));
        assert!(!scene.filter_active);
        assert!(scene.filter_pattern.is_empty());
        assert_eq!(scene.filtered_entry_count(), 3);
    }

    #[test]
    fn hidden_toggle_updates_filtered_projection_and_prunes_selection() {
        let mut scene = test_scene(
            vec![
                test_entry("alpha.txt", false),
                test_entry(".secret", false),
                test_entry("bravo.txt", false),
            ],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(420, 260);

        assert_eq!(
            scene.panes[ShellPaneId::SLOT_0].filtered_indexes,
            vec![0, 2]
        );
        assert_eq!(scene.filtered_entry_count(), 2);
        assert!(
            scene.panes[ShellPaneId::SLOT_0]
                .selection
                .apply_navigation(1, false)
        );

        assert!(scene.toggle_hidden_visibility(size));
        assert!(scene.show_hidden);
        assert_eq!(
            scene.panes[ShellPaneId::SLOT_0].filtered_indexes,
            vec![0, 1, 2]
        );
        assert_eq!(scene.hidden_changes, 1);
        assert!(scene.panes[ShellPaneId::SLOT_0].selection.contains(1));

        assert!(scene.toggle_hidden_visibility(size));
        assert!(!scene.show_hidden);
        assert_eq!(
            scene.panes[ShellPaneId::SLOT_0].filtered_indexes,
            vec![0, 2]
        );
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].selection.len(), 0);
        assert_eq!(scene.selection_changes, 1);
    }

    #[test]
    fn app_toolbar_does_not_expose_temporary_mouse_buttons() {
        let scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(420, 240);
        let toolbar_point = ViewPoint {
            x: 18.0,
            y: scene.app_toolbar_y() + 18.0,
        };

        assert_eq!(scene.view_mode_at_screen_point(toolbar_point, size), None);
        assert_eq!(
            scene.path_navigation_action_at_screen_point(toolbar_point, size),
            None
        );
    }

    #[test]
    fn switching_view_modes_clamps_scroll_and_refreshes_layout_axis() {
        let mut scene = test_scene(
            (0..30)
                .map(|index| test_entry(&format!("entry-{index}.txt"), false))
                .collect(),
            ShellViewMode::Compact,
        );
        let size = PhysicalSize::new(260, 180);
        scene.panes[ShellPaneId::SLOT_0].scroll_x = 10_000.0;
        scene.panes[ShellPaneId::SLOT_0].scroll_y = 500.0;
        scene.rubber_band = Some(RubberBand::new(
            ViewPoint { x: 0.0, y: 0.0 },
            RubberBandMode::Replace,
            ShellSelection::default(),
        ));

        assert!(scene.set_view_mode(ShellViewMode::Details, size));
        assert_eq!(
            scene.panes[ShellPaneId::SLOT_0].view_mode,
            ShellViewMode::Details
        );
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].scroll_x, 0.0);
        assert!(scene.panes[ShellPaneId::SLOT_0].scroll_y >= 0.0);
        assert!(scene.rubber_band.is_none());
        assert_eq!(scene.view_switches, 1);

        assert!(!scene.set_view_mode(ShellViewMode::Details, size));
        assert_eq!(scene.view_switches, 1);
    }

    #[test]
    fn switching_view_modes_only_changes_active_pane() {
        let mut scene = test_scene(
            (0..80)
                .map(|index| test_entry(&format!("left-{index:02}.txt"), false))
                .collect(),
            ShellViewMode::Icons,
        );
        set_test_pane(
            &mut scene,
            ShellPaneId::SLOT_1,
            PathBuf::from("/right-root"),
            ShellViewMode::Details,
            vec![test_entry("right.txt", false)],
        );
        let size = PhysicalSize::new(900, 360);
        scene.panes[ShellPaneId::SLOT_0].scroll_y = 18.0;
        scene.panes[ShellPaneId::SLOT_1].scroll_y = 42.0;
        scene.active_pane = ShellPaneId::SLOT_1;

        assert!(scene.set_view_mode(ShellViewMode::Compact, size));
        assert_eq!(
            scene.panes[ShellPaneId::SLOT_0].view_mode,
            ShellViewMode::Icons
        );
        assert_eq!(
            scene.panes[ShellPaneId::SLOT_1].view_mode,
            ShellViewMode::Compact
        );
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].scroll_y, 18.0);
        assert_eq!(scene.panes[ShellPaneId::SLOT_1].scroll_y, 0.0);
        assert_eq!(scene.view_switches, 1);

        assert!(!scene.set_view_mode(ShellViewMode::Compact, size));
        scene.active_pane = ShellPaneId::SLOT_0;
        assert!(scene.set_view_mode(ShellViewMode::Details, size));
        assert_eq!(
            scene.panes[ShellPaneId::SLOT_0].view_mode,
            ShellViewMode::Details
        );
        assert_eq!(
            scene.panes[ShellPaneId::SLOT_1].view_mode,
            ShellViewMode::Compact
        );
        assert_eq!(scene.view_switches, 2);
    }

    #[test]
    fn zooming_only_changes_active_pane_and_its_layout_metrics() {
        let mut scene = test_scene(
            (0..80)
                .map(|index| test_entry(&format!("left-{index:02}.txt"), false))
                .collect(),
            ShellViewMode::Icons,
        );
        set_test_pane(
            &mut scene,
            ShellPaneId::SLOT_1,
            PathBuf::from("/right-root"),
            ShellViewMode::Icons,
            (0..80)
                .map(|index| test_entry(&format!("right-{index:02}.txt"), false))
                .collect(),
        );
        let size = PhysicalSize::new(900, 420);
        let left_before = scene
            .pane_projection(ShellPaneId::SLOT_0, size)
            .unwrap()
            .visible_items[0]
            .layout
            .icon_rect
            .width;
        let right_before = scene
            .pane_projection(ShellPaneId::SLOT_1, size)
            .unwrap()
            .visible_items[0]
            .layout
            .icon_rect
            .width;

        scene.active_pane = ShellPaneId::SLOT_1;
        assert!(scene.zoom(ZoomAction::In, size));
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].zoom_level(), 4);
        assert_eq!(scene.panes[ShellPaneId::SLOT_1].zoom_level(), 5);
        assert_eq!(scene.zoom_percent_for_pane(ShellPaneId::SLOT_0), 100);
        assert!(scene.zoom_percent_for_pane(ShellPaneId::SLOT_1) > 100);

        let left_after = scene
            .pane_projection(ShellPaneId::SLOT_0, size)
            .unwrap()
            .visible_items[0]
            .layout
            .icon_rect
            .width;
        let right_after = scene
            .pane_projection(ShellPaneId::SLOT_1, size)
            .unwrap()
            .visible_items[0]
            .layout
            .icon_rect
            .width;
        assert_eq!(left_after, left_before);
        assert!(right_after > right_before);
    }

    #[test]
    fn zoom_updates_layout_metrics_for_all_view_modes() {
        let mut scene = test_scene(
            (0..80)
                .map(|index| test_entry(&format!("entry-{index}.txt"), index % 5 == 0))
                .collect(),
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(420, 260);
        let icons_before = match scene.layout(size) {
            ShellLayout::Icons(layout) => layout.item(0).unwrap(),
            _ => unreachable!(),
        };

        assert!(scene.zoom(ZoomAction::In, size));
        assert_eq!(scene.zoom_changes, 1);
        assert!(scene.zoom_percent_for_pane(scene.active_pane()) > 100);
        let icons_after = match scene.layout(size) {
            ShellLayout::Icons(layout) => layout.item(0).unwrap(),
            _ => unreachable!(),
        };
        assert!(icons_after.icon_rect.width > icons_before.icon_rect.width);
        assert!(icons_after.item_rect.height > icons_before.item_rect.height);
        assert_eq!(
            scene.icons_options(size).text_height,
            scene.text_line_height()
        );

        assert!(scene.set_view_mode(ShellViewMode::Compact, size));
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].zoom_level(), 3);
        assert!(scene.zoom(ZoomAction::In, size));
        let compact_zoomed = match scene.layout(size) {
            ShellLayout::Compact(layout) => layout.item(0).unwrap(),
            _ => unreachable!(),
        };
        assert!(scene.zoom(ZoomAction::Reset, size));
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].zoom_level(), 0);
        let compact_reset = match scene.layout(size) {
            ShellLayout::Compact(layout) => layout.item(0).unwrap(),
            _ => unreachable!(),
        };
        assert!(compact_zoomed.icon_rect.width > compact_reset.icon_rect.width);
        assert!(compact_zoomed.item_rect.height > compact_reset.item_rect.height);
        assert_eq!(
            scene.compact_options(size).text_height,
            scene.text_line_height()
        );

        assert!(scene.set_view_mode(ShellViewMode::Icons, size));
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].zoom_level(), 5);

        assert!(scene.set_view_mode(ShellViewMode::Details, size));
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].zoom_level(), 3);
        let details_before = match scene.layout(size) {
            ShellLayout::Details(layout) => layout.item(0).unwrap(),
            _ => unreachable!(),
        };
        assert!(scene.zoom(ZoomAction::Out, size));
        let details_after = match scene.layout(size) {
            ShellLayout::Details(layout) => layout.item(0).unwrap(),
            _ => unreachable!(),
        };
        assert!(details_after.item_rect.height <= details_before.item_rect.height);
        assert!(details_after.icon_rect.width < details_before.icon_rect.width);

        assert!(scene.zoom(ZoomAction::Reset, size));
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].zoom_level(), 0);
    }

    #[test]
    fn icons_item_width_matches_file_manager_update_grid_size_formula() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(420, 260);
        let options = scene.icons_options(size);
        let zoom_level = scene.panes[ShellPaneId::SLOT_0].zoom_level();
        let expected = file_manager_icons_item_width(
            64.0,
            2.0,
            FILE_MANAGER_ICONS_TEXT_WIDTH_INDEX,
            9.0,
            1.0,
            zoom_level,
        );

        assert_eq!(FILE_MANAGER_ICONS_TEXT_WIDTH_INDEX, 1.0);
        assert_eq!(options.item_width, expected);
        assert_eq!(options.item_width, 103.0);

        assert!(scene.set_scale_factor(1.5, size));
        let scaled_options = scene.icons_options(size);
        let scaled_expected = file_manager_icons_item_width(
            96.0,
            3.0,
            FILE_MANAGER_ICONS_TEXT_WIDTH_INDEX,
            13.5,
            1.5,
            zoom_level,
        );
        assert_eq!(scaled_options.item_width, scaled_expected);
        assert_eq!(scaled_options.item_width, 154.0);
    }

    #[test]
    fn status_zoom_indicator_uses_shared_geometry_for_hit_tests_and_cursor() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(760, 520);
        let rects = scene
            .status_zoom_indicator_rects_for_pane(ShellPaneId::SLOT_0, size)
            .expect("wide status bar should show zoom indicator");
        let point = ViewPoint {
            x: rects.track.x + rects.track.width / 2.0,
            y: rects.track.y + rects.track.height / 2.0,
        };

        assert!(scene.scrollbar_drag_hit_at_screen_point(point, size));
        assert_eq!(
            scene.status_zoom_indicator_rects_at_screen_point(point, size),
            Some((ShellPaneId::SLOT_0, rects))
        );
        let _ = scene.set_pointer(point, size);
        assert_eq!(scene.cursor_icon(size), CursorIcon::Pointer);
        assert_eq!(scene.hit_test_screen_point(point, size), None);
    }

    #[test]
    fn status_zoom_indicator_drag_sets_discrete_zoom_level() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(760, 520);
        let rects = scene
            .status_zoom_indicator_rects_for_pane(ShellPaneId::SLOT_0, size)
            .expect("wide status bar should show zoom indicator");
        let high = ViewPoint {
            x: rects.track.right() - 1.0,
            y: rects.track.y + rects.track.height / 2.0,
        };

        assert_eq!(scene.panes[ShellPaneId::SLOT_0].zoom_level(), 4);
        assert_eq!(scene.begin_scrollbar_drag(high, size), Some(true));
        assert_eq!(
            scene.scrollbar_drag.map(|drag| drag.target),
            Some(ScrollbarDragTarget::StatusZoom {
                pane: ShellPaneId::SLOT_0
            })
        );
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].zoom_level(), 16);
        assert_eq!(scene.cursor_icon(size), CursorIcon::Pointer);

        let low = ViewPoint {
            x: rects.track.x,
            y: rects.track.y + rects.track.height / 2.0,
        };
        assert!(scene.set_pointer(low, size));
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].zoom_level(), 0);
        let _ = scene.end_scrollbar_drag(low, size);
        assert!(scene.scrollbar_drag.is_none());

        assert!(scene.set_zoom_level(ShellPaneId::SLOT_0, 16, size, true));
        let label = ViewPoint {
            x: rects.label.x + rects.label.width / 2.0,
            y: rects.label.y + rects.label.height / 2.0,
        };
        assert_eq!(scene.begin_scrollbar_drag(label, size), Some(true));
        // DolphinView::resetZoomLevel() uses the mode IconSize setting, not
        // PreviewSize: the Icons default is 32px / ZoomLevelInfo level 2.
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].zoom_level(), 2);
        assert!(scene.scrollbar_drag.is_none());
    }

    #[test]
    fn content_scrollbar_thumb_drag_reports_debounced_visible_range_updates() {
        let mut scene = test_scene(
            (0..240)
                .map(|index| test_entry(&format!("tool-{index:03}"), false))
                .collect(),
            ShellViewMode::Compact,
        );
        scene.places_visible = false;
        let size = PhysicalSize::new(760, 520);
        let pane = ShellPaneId::SLOT_0;
        let (track, thumb) = scene
            .pane_content_scrollbar_rects(pane, size)
            .expect("compact /bin-sized model should expose a horizontal scrollbar");
        let start = ViewPoint {
            x: thumb.x + thumb.width / 2.0,
            y: thumb.y + thumb.height / 2.0,
        };

        assert_eq!(scene.begin_scrollbar_drag(start, size), Some(false));
        assert_eq!(
            scene.scrollbar_drag_visible_role_update_kind(),
            Some(crate::ui::prewarm::VisibleRoleUpdateKind::VisibleRange)
        );
        let end = ViewPoint {
            x: track.right() - 1.0,
            y: start.y,
        };
        assert!(scene.set_pointer(end, size));
        assert!(scene.panes[pane].scroll_x > 0.0);
        let _ = scene.end_scrollbar_drag(end, size);
        assert_eq!(scene.scrollbar_drag_visible_role_update_kind(), None);
    }

    #[test]
    fn held_content_scrollbar_thumb_resolves_the_settled_visible_page() {
        let root = test_dir("held-scrollbar-visible-mime");
        std::fs::create_dir_all(&root).unwrap();
        let target_name = "zzz-extensionless-image";
        std::fs::write(
            root.join(target_name),
            b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR",
        )
        .unwrap();

        let mut entries = (0..239)
            .map(|index| test_entry(&format!("tool-{index:03}"), false))
            .collect::<Vec<_>>();
        entries.push(test_unchecked_generic_entry(target_name, 24, 0));
        let mut scene = TestShellSceneBuilder::new()
            .with_path(root.clone())
            .with_entries(entries)
            .with_view_mode(ShellViewMode::Compact)
            .with_places_visible(false)
            .build();
        let size = PhysicalSize::new(760, 520);
        let pane = ShellPaneId::SLOT_0;
        let (track, thumb) = scene
            .pane_content_scrollbar_rects(pane, size)
            .expect("compact model should expose a horizontal scrollbar");
        let start = ViewPoint {
            x: thumb.x + thumb.width / 2.0,
            y: thumb.y + thumb.height / 2.0,
        };
        assert_eq!(scene.begin_scrollbar_drag(start, size), Some(false));
        assert!(scene.set_pointer(
            ViewPoint {
                x: track.right() - 1.0,
                y: start.y,
            },
            size,
        ));

        let timer_start = Instant::now();
        let mut updates = crate::ui::prewarm::VisibleRoleUpdateState::default();
        updates.schedule_visible_range(timer_start);
        assert_eq!(
            synchronize_visible_metadata_roles(&mut scene, size, updates.role_updates_paused())
                .applied,
            0
        );
        assert_eq!(
            updates.take_due_update(
                timer_start
                    + Duration::from_millis(FILE_MANAGER_VISIBLE_RANGE_UPDATE_MS),
            ),
            Some(crate::ui::prewarm::VisibleRoleUpdateKind::VisibleRange)
        );
        assert!(scene.is_scrollbar_dragging());

        let settled = synchronize_visible_metadata_roles(
            &mut scene,
            size,
            updates.role_updates_paused(),
        );
        let target = scene.panes[pane]
            .entries
            .iter()
            .find(|entry| entry.name.as_ref() == target_name)
            .expect("target entry");
        assert!(settled.visible > 0);
        assert_eq!(settled.applied, 1);
        assert!(target.mime_magic_checked);
        assert_eq!(target.mime_type.as_deref(), Some("image/png"));

        std::fs::remove_dir_all(root).unwrap();
    }
