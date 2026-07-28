
    #[test]
    fn places_chrome_starts_at_pane_origin_below_app_toolbar() {
        let scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let pane = scene.pane_rect(size);
        let sidebar = scene.places_sidebar_rect(size);
        let panel = scene.places_panel_rect(size);

        assert_eq!(sidebar.y, pane.y);
        assert_eq!(panel.y, pane.y);
        assert_eq!(sidebar.bottom(), pane.bottom());
        assert!(
            scene.content_origin_x(size) > sidebar.right(),
            "file pane should leave a visual gap after Places"
        );
        assert_eq!(
            scene.content_origin_x(size) - sidebar.right(),
            scene.scale_metric(PLACES_SIDEBAR_SPLITTER_WIDTH)
                + scene.scale_metric(PLACES_TO_PANE_GAP)
        );
        assert!(
            panel.right() < sidebar.right(),
            "Places panel should keep right padding inside the sidebar"
        );
        assert!(scene.app_toolbar_height() < pane.y);
        assert!(!sidebar.contains(ViewPoint {
            x: sidebar.x + 8.0,
            y: scene.app_toolbar_height() / 2.0,
        }));
    }

    #[test]
    fn places_toggle_hides_sidebar_and_reclaims_pane_width() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let before_origin = scene.content_origin_x(size);
        let before_width = scene.pane_width(size);
        let place_row = scene.place_row_rects(size)[0].1;
        let place_point = ViewPoint {
            x: place_row.x + 4.0,
            y: place_row.y + 4.0,
        };
        let toggle = scene.places_toggle_rect(size);

        assert_eq!(
            scene.toggle_places_at_screen_point(
                ViewPoint {
                    x: toggle.x + 2.0,
                    y: toggle.y + 2.0,
                },
                size,
            ),
            Some(true)
        );
        assert!(!scene.places_visible);
        assert_eq!(scene.places_sidebar_width(size), 0.0);
        assert_eq!(scene.content_origin_x(size), 0.0);
        assert!(scene.pane_width(size) > before_width);
        assert_eq!(scene.place_index_at_screen_point(place_point, size), None);
        assert_eq!(scene.places_changes, 1);

        assert_eq!(
            scene.toggle_places_at_screen_point(
                ViewPoint {
                    x: toggle.x + 2.0,
                    y: toggle.y + 2.0,
                },
                size,
            ),
            Some(true)
        );
        assert!(scene.places_visible);
        assert_eq!(scene.content_origin_x(size), before_origin);
        assert!(scene.places_sidebar_width(size) > 0.0);
    }

    #[test]
    fn overflow_button_is_right_aligned_after_split_view_button() {
        let scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let toolbar = scene.app_toolbar_rect(size);
        let button = scene.split_view_button_rect(size);
        let overflow = scene.overflow_button_rect(size);
        let places = scene.places_toggle_rect(size);
        let point = ViewPoint {
            x: button.x + button.width / 2.0,
            y: button.y + button.height / 2.0,
        };

        assert!(overflow.right() <= toolbar.right() - scene.scale_metric(8.0) + 0.5);
        assert!(button.right() < overflow.x);
        assert!(button.x > toolbar.width / 2.0);
        assert!(button.x > places.right());
        let toolbar_center_y = toolbar.y + toolbar.height / 2.0;
        assert!((button.y + button.height / 2.0 - toolbar_center_y).abs() < 0.001);
        assert!((overflow.y + overflow.height / 2.0 - toolbar_center_y).abs() < 0.001);
        assert!((places.y + places.height / 2.0 - toolbar_center_y).abs() < 0.001);
        assert!(scene.split_view_button_at_screen_point(point, size));
        assert!(scene.overflow_button_contains_screen_point(
            ViewPoint {
                x: overflow.x + overflow.width / 2.0,
                y: overflow.y + overflow.height / 2.0,
            },
            size,
        ));
        assert!(!scene.split_view_button_at_screen_point(
            ViewPoint {
                x: places.x + places.width / 2.0,
                y: places.y + places.height / 2.0,
            },
            size,
        ));
    }

    #[test]
    fn settings_dialog_routes_actions_and_updates_background_opacity() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = shell::settings::settings_dialog_window_size_scaled(scene.ui_scale());
        let hidden_row = shell::settings::settings_dialog_row_rect(
            size,
            scene.ui_scale(),
            0,
        )
        .unwrap();
        assert_eq!(
            settings_action_at_screen_point(
                ViewPoint {
                    x: hidden_row.x + 8.0,
                    y: hidden_row.y + hidden_row.height / 2.0,
                },
                size,
                scene.ui_scale(),
            ),
            Some(ShellSettingsAction::ToggleHiddenFiles)
        );
        let track = shell::settings::settings_dialog_opacity_track_rect(size, scene.ui_scale());
        assert_eq!(
            settings_action_at_screen_point(
                ViewPoint {
                    x: track.x + track.width / 2.0,
                    y: track.y,
                },
                size,
                scene.ui_scale(),
            ),
            Some(ShellSettingsAction::SetBackgroundOpacity(60))
        );
        assert!(scene.set_background_opacity_percent(60));
        assert_eq!(scene.background_opacity, 0.6);
    }

    #[test]
    fn toolbar_view_mode_segments_select_specific_modes_and_use_pointer_cursor() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let control = scene
            .app_toolbar_layout(size)
            .view_mode
            .expect("wide toolbar should expose a view mode control");
        let toolbar = scene.app_toolbar_rect(size);
        let toolbar_center_y = toolbar.y + toolbar.height / 2.0;
        assert!((control.outer.y + control.outer.height / 2.0 - toolbar_center_y).abs() < 0.001);
        for segment in control.segments {
            assert!((segment.rect.y + segment.rect.height / 2.0 - toolbar_center_y).abs() < 0.001);
        }
        let compact = control
            .segments
            .into_iter()
            .find(|segment| segment.mode == ShellViewMode::Compact)
            .expect("compact segment");
        let compact_point = ViewPoint {
            x: compact.rect.x + compact.rect.width / 2.0,
            y: compact.rect.y + compact.rect.height / 2.0,
        };

        assert_eq!(
            scene.view_mode_at_screen_point(compact_point, size),
            Some(ShellViewMode::Compact)
        );
        let _ = scene.set_pointer(compact_point, size);
        assert_eq!(scene.cursor_icon(size), CursorIcon::Pointer);

        let next = scene
            .view_mode_at_screen_point(compact_point, size)
            .unwrap();
        assert!(scene.set_view_mode(next, size));
        assert_eq!(scene.active_view_mode(), ShellViewMode::Compact);
        let details = scene
            .app_toolbar_layout(size)
            .view_mode
            .unwrap()
            .segments
            .into_iter()
            .find(|segment| segment.mode == ShellViewMode::Details)
            .unwrap();
        let details_point = ViewPoint {
            x: details.rect.x + details.rect.width / 2.0,
            y: details.rect.y + details.rect.height / 2.0,
        };
        assert_eq!(
            scene.view_mode_at_screen_point(details_point, size),
            Some(ShellViewMode::Details)
        );
    }

    #[test]
    fn places_resize_drag_updates_sidebar_width_and_content_origin() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let before_width = scene.places_sidebar_width(size);
        let before_origin = scene.content_origin_x(size);
        let handle = scene.places_resize_handle_rect(size).unwrap();
        let start = ViewPoint {
            x: handle.x + handle.width / 2.0,
            y: handle.y + 12.0,
        };

        assert!(scene.begin_scrollbar_drag(start, size).is_some());
        assert!(scene.is_scrollbar_dragging());
        assert!(scene.set_pointer(
            ViewPoint {
                x: start.x + 48.0,
                y: start.y,
            },
            size
        ));
        assert!(scene.places_sidebar_width(size) > before_width);
        assert!(scene.content_origin_x(size) > before_origin);
        assert_eq!(scene.places_resize_changes, 1);

        assert!(scene.set_pointer(ViewPoint { x: 0.0, y: start.y }, size));
        assert_eq!(
            scene.places_sidebar_width(size),
            scene.places_sidebar_width_bounds(size).0
        );
        assert!(scene.places_resize_changes >= 2);
        let _ = scene.end_scrollbar_drag(ViewPoint { x: 0.0, y: start.y }, size);
        assert!(!scene.is_scrollbar_dragging());
    }

    #[test]
    fn places_resize_handle_is_left_biased_and_easier_to_hit() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let sidebar = scene.places_sidebar_rect(size);
        let handle = scene.places_resize_handle_rect(size).unwrap();
        let expected_left = sidebar.right() - scene.scale_metric(PLACES_RESIZE_HANDLE_WIDTH);

        assert!(handle.x <= expected_left + 0.5);
        assert!(handle.right() > sidebar.right());

        let left_edge_point = ViewPoint {
            x: handle.x + 0.25,
            y: handle.y + 12.0,
        };
        let _ = scene.set_pointer(left_edge_point, size);
        assert_eq!(scene.cursor_icon(size), CursorIcon::ColResize);

        let outside_left = ViewPoint {
            x: handle.x - 1.0,
            y: handle.y + 12.0,
        };
        let _ = scene.set_pointer(outside_left, size);
        assert_eq!(scene.cursor_icon(size), CursorIcon::Default);
    }

    #[test]
    fn address_bar_uses_text_cursor_without_overriding_resize_cursor() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let path_bar = scene.path_bar_rect(size).unwrap();
        let path_point = ViewPoint {
            x: path_bar.x + path_bar.width / 2.0,
            y: path_bar.y + path_bar.height / 2.0,
        };

        let _ = scene.set_pointer(path_point, size);
        assert_eq!(scene.cursor_icon(size), CursorIcon::Text);

        let handle = scene.places_resize_handle_rect(size).unwrap();
        let handle_point = ViewPoint {
            x: handle.x + 1.0,
            y: handle.y + 12.0,
        };
        let _ = scene.set_pointer(handle_point, size);
        assert_eq!(scene.cursor_icon(size), CursorIcon::ColResize);
        assert!(scene.begin_scrollbar_drag(handle_point, size).is_some());
        let _ = scene.set_pointer(path_point, size);
        assert_eq!(scene.cursor_icon(size), CursorIcon::ColResize);
    }

    #[test]
    fn splitter_cursor_hints_follow_hover_and_drag_state() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let handle = scene.places_resize_handle_rect(size).unwrap();
        let point = ViewPoint {
            x: handle.x + handle.width / 2.0,
            y: handle.y + 10.0,
        };

        assert_eq!(scene.cursor_icon(size), CursorIcon::Default);
        let _ = scene.set_pointer(point, size);
        assert_eq!(scene.cursor_icon(size), CursorIcon::ColResize);
        assert!(scene.begin_scrollbar_drag(point, size).is_some());
        assert_eq!(scene.cursor_icon(size), CursorIcon::ColResize);
        assert!(scene.set_pointer(
            ViewPoint {
                x: point.x + 30.0,
                y: point.y,
            },
            size
        ));
        assert_eq!(scene.cursor_icon(size), CursorIcon::ColResize);
        let _ = scene.end_scrollbar_drag(
            ViewPoint {
                x: point.x + 30.0,
                y: point.y,
            },
            size,
        );
        assert_eq!(scene.cursor_icon(size), CursorIcon::ColResize);
        let _ = scene.set_pointer(
            ViewPoint {
                x: scene.content_origin_x(size) + scene.content_width(size) - 8.0,
                y: scene.content_origin_y() + 8.0,
            },
            size,
        );
        assert_eq!(scene.cursor_icon(size), CursorIcon::Default);
    }

    #[test]
    fn split_pane_divider_drag_updates_left_width_and_cursor_hint() {
        let mut scene = test_scene(vec![test_entry("alpha", true)], ShellViewMode::Icons);
        let split_entries = vec![test_entry("right", true)];
        scene.panes.set(
            ShellPaneId::SLOT_1,
            ShellPaneState {
                path: PathBuf::from("/right-root"),
                pending_path: None,
                view_mode: ShellViewMode::Icons,
                zoom_step: 0,
                dir_count: 1,
                filtered_indexes: filtered_indexes_for_entries(&split_entries, false, ""),
                entries: split_entries,
                selection: ShellSelection::default(),
                scroll_x: 0.0,
                scroll_y: 0.0,
            },
        );
        let size = PhysicalSize::new(900, 360);
        let before_left = scene.split_pane_metrics(size).unwrap().left_width;
        let handle = scene.split_pane_resize_handle_rect(size).unwrap();
        let start = ViewPoint {
            x: handle.x + handle.width / 2.0,
            y: handle.y + 20.0,
        };

        let _ = scene.set_pointer(start, size);
        assert_eq!(scene.cursor_icon(size), CursorIcon::ColResize);
        assert!(scene.begin_scrollbar_drag(start, size).is_some());
        assert!(scene.set_pointer(
            ViewPoint {
                x: start.x + 70.0,
                y: start.y,
            },
            size
        ));
        let after_left = scene.split_pane_metrics(size).unwrap().left_width;
        assert!(after_left > before_left);
        assert!(scene.split_pane_left_fraction > 0.5);
        assert_eq!(scene.cursor_icon(size), CursorIcon::ColResize);

        assert!(scene.set_pointer(ViewPoint { x: 0.0, y: start.y }, size));
        let min_left = scene.split_pane_width_bounds(size).unwrap().1;
        assert_eq!(scene.split_pane_metrics(size).unwrap().left_width, min_left);
        let _ = scene.end_scrollbar_drag(ViewPoint { x: 0.0, y: start.y }, size);
        assert!(!scene.is_scrollbar_dragging());
    }

    #[test]
    fn hidden_places_do_not_capture_scroll() {
        let entries = (0..80)
            .map(|index| test_entry(&format!("entry-{index:02}.txt"), false))
            .collect::<Vec<_>>();
        let mut scene = test_scene(entries, ShellViewMode::Icons);
        scene.places = (0..28)
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
        let size = PhysicalSize::new(700, 220);
        assert!(scene.max_places_scroll_y(size) > 0.0);
        assert_eq!(
            scene.toggle_places_at_screen_point(
                ViewPoint {
                    x: scene.places_toggle_rect(size).x + 2.0,
                    y: scene.places_toggle_rect(size).y + 2.0,
                },
                size,
            ),
            Some(true)
        );

        scene.pointer = Some(ViewPoint {
            x: PLACES_SIDEBAR_PADDING_X + 2.0,
            y: scene.content_origin_y() + 10.0,
        });
        assert!(scene.scroll_by(90.0, size));
        assert_eq!(scene.places_scroll_y, 0.0);
        assert!(scene.panes[ShellPaneId::SLOT_0].scroll_y > 0.0);
    }

    #[test]
    fn drop_target_lookup_resolves_places_slot0_blank_and_split_items() {
        let mut scene = test_scene(
            vec![test_entry("alpha", true), test_entry("note.txt", false)],
            ShellViewMode::Icons,
        );
        let split_entries = vec![test_entry("right", true)];
        scene.panes.set(
            ShellPaneId::SLOT_1,
            ShellPaneState {
                path: PathBuf::from("/right-root"),
                pending_path: None,
                view_mode: ShellViewMode::Icons,
                zoom_step: 0,
                dir_count: 1,
                filtered_indexes: filtered_indexes_for_entries(&split_entries, false, ""),
                entries: split_entries,
                selection: ShellSelection::default(),
                scroll_x: 0.0,
                scroll_y: 0.0,
            },
        );
        let size = PhysicalSize::new(900, 360);

        let place_row = scene.place_row_rects(size)[0].1;
        assert_eq!(
            scene.drop_target_at_screen_point(
                ViewPoint {
                    x: place_row.x + 4.0,
                    y: place_row.y + 4.0,
                },
                size,
            ),
            Some(ShellDropTarget::Place {
                index: 0,
                path: PathBuf::from("/tmp"),
            })
        );

        let places_panel = scene.places_panel_rect(size);
        assert_eq!(
            scene.drop_target_at_screen_point(
                ViewPoint {
                    x: places_panel.x + 4.0,
                    y: places_panel.y + 4.0,
                },
                size,
            ),
            Some(ShellDropTarget::PlacesBlank)
        );

        let left_geometry = scene.pane_geometry(ShellPaneId::SLOT_0, size).unwrap();
        let left_item = scene.layout(size).item(0).unwrap();
        assert_eq!(
            scene.drop_target_at_screen_point(
                ViewPoint {
                    x: left_geometry.content.x + left_item.visual_rect.x + 4.0,
                    y: left_geometry.content.y + left_item.visual_rect.y + 4.0,
                },
                size,
            ),
            Some(ShellDropTarget::PaneItem {
                pane: ShellPaneId::SLOT_0,
                index: 0,
                path: PathBuf::from("/tmp/alpha"),
                is_dir: true,
            })
        );
        assert_eq!(
            scene.drop_target_at_screen_point(
                ViewPoint {
                    x: left_geometry.content.right() - 2.0,
                    y: left_geometry.content.bottom() - 2.0,
                },
                size,
            ),
            Some(ShellDropTarget::PaneBlank {
                pane: ShellPaneId::SLOT_0,
                path: PathBuf::from("/tmp"),
            })
        );

        let split_geometry = scene.pane_geometry(ShellPaneId::SLOT_1, size).unwrap();
        let split_view = scene.pane_view(ShellPaneId::SLOT_1).unwrap();
        let split_layout = scene.pane_layout_for_pane(
            ShellPaneId::SLOT_1,
            split_view,
            split_geometry.content.width,
            split_geometry.content.height,
        );
        let split_item = split_layout.item(0).unwrap();
        assert_eq!(
            scene.drop_target_at_screen_point(
                ViewPoint {
                    x: split_geometry.content.x + split_item.visual_rect.x + 4.0,
                    y: split_geometry.content.y + split_item.visual_rect.y + 4.0,
                },
                size,
            ),
            Some(ShellDropTarget::PaneItem {
                pane: ShellPaneId::SLOT_1,
                index: 0,
                path: PathBuf::from("/right-root/right"),
                is_dir: true,
            })
        );
    }
