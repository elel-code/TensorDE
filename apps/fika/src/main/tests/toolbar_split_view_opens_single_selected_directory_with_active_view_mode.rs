
    #[test]
    fn toolbar_split_view_opens_single_selected_directory_with_active_view_mode() {
        let root = test_dir("toolbar-split-selected-dir");
        let child = root.join("child");
        fs::create_dir_all(&child).unwrap();
        fs::write(child.join("nested.txt"), b"nested").unwrap();
        fs::write(root.join("plain.txt"), b"plain").unwrap();

        let size = PhysicalSize::new(900, 420);
        let mut scene =
            ShellScene::load_with_hidden_visibility(root.clone(), ShellViewMode::Details, false)
                .unwrap();
        let child_index =
            entry_index_by_name(&scene.panes[ShellPaneId::SLOT_0].entries, "child").unwrap();
        assert!(
            scene.panes[ShellPaneId::SLOT_0]
                .selection
                .apply_navigation(child_index, false)
        );

        assert!(scene.toggle_split_view_from_toolbar(size).unwrap());
        let split = scene
            .panes
            .get(ShellPaneId::SLOT_1)
            .expect("split pane should load");
        assert_eq!(split.path, child);
        assert_eq!(split.view_mode, ShellViewMode::Details);
        assert_eq!(split.entries.len(), 1);
        assert_eq!(split.entries[0].name.as_ref(), "nested.txt");
        assert_eq!(scene.active_pane(), ShellPaneId::SLOT_1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn toolbar_split_view_closes_current_pane_and_keeps_the_other_pane() {
        let root = test_dir("toolbar-split-close-current");
        let left = root.join("left");
        let right = root.join("right");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        fs::write(left.join("left.txt"), b"left").unwrap();
        fs::write(right.join("right.txt"), b"right").unwrap();

        let size = PhysicalSize::new(900, 420);
        let mut scene = ShellScene::load(left.clone(), ShellViewMode::Icons).unwrap();
        assert!(scene.open_split_pane(right.clone(), size).unwrap());
        assert_eq!(scene.active_pane(), ShellPaneId::SLOT_1);

        assert!(scene.toggle_split_view_from_toolbar(size).unwrap());
        assert!(!scene.panes.is_open(ShellPaneId::SLOT_1));
        assert_eq!(scene.active_pane(), ShellPaneId::SLOT_0);
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].path, left);

        assert!(scene.open_split_pane(right.clone(), size).unwrap());
        scene.active_pane = ShellPaneId::SLOT_0;
        assert!(scene.toggle_split_view_from_toolbar(size).unwrap());
        assert!(!scene.panes.is_open(ShellPaneId::SLOT_1));
        assert_eq!(scene.active_pane(), ShellPaneId::SLOT_0);
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].path, right);
        assert_eq!(
            scene.panes[ShellPaneId::SLOT_0].entries[0].name.as_ref(),
            "right.txt"
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn open_with_chooser_opens_from_file_context_and_filters_applications() {
        let mut scene = test_scene(vec![test_entry("note.txt", false)], ShellViewMode::Icons);
        scene.context_target = Some(ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: PathBuf::from("/tmp/note.txt"),
            is_dir: false,
            selection_count: 1,
        });
        let list = fika_core::parse_mimeapps_list(
            "\
[Default Applications]\n\
text/plain=writer.desktop;\n",
        );
        let cache = MimeApplicationCache::from_applications_and_mimeapps(
            vec![
                test_desktop_application("viewer.desktop", "Viewer", "viewer %f", &["text/plain"]),
                test_desktop_application("writer.desktop", "Writer", "writer %f", &["text/plain"]),
                test_desktop_application("paint.desktop", "Paint", "paint %f", &["image/png"]),
            ],
            &[list],
        );

        assert!(scene.open_open_with_chooser_from_context(&cache));
        let chooser = scene
            .open_with_chooser
            .as_ref()
            .expect("chooser should open");
        assert_eq!(chooser.path, PathBuf::from("/tmp/note.txt"));
        assert_eq!(chooser.mime_type.as_deref(), Some("text/plain"));
        assert_eq!(
            chooser
                .applications
                .iter()
                .map(|application| application.id.as_str())
                .collect::<Vec<_>>(),
            vec!["writer.desktop", "viewer.desktop", "paint.desktop"]
        );
        assert_eq!(
            chooser.selected_application().map(|app| app.id.as_str()),
            None
        );

        assert!(scene.apply_open_with_command(OpenWithCommand::Insert("paint".to_string())));
        let chooser = scene.open_with_chooser.as_ref().unwrap();
        assert_eq!(chooser.filtered_count(), 1);
        assert_eq!(
            chooser.selected_application().map(|app| app.id.as_str()),
            Some("paint.desktop")
        );
    }

    #[test]
    fn open_with_chooser_opens_from_blank_directory_context() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.context_target = Some(ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_0,
            path: PathBuf::from("/tmp"),
        });
        let cache = MimeApplicationCache::from_applications_and_mimeapps(
            vec![test_desktop_application(
                "code.desktop",
                "Code",
                "code %F",
                &["inode/directory"],
            )],
            &[],
        );

        assert!(scene.open_open_with_chooser_from_context(&cache));
        let chooser = scene
            .open_with_chooser
            .as_ref()
            .expect("directory chooser should open");
        assert_eq!(chooser.path, PathBuf::from("/tmp"));
        assert_eq!(chooser.mime_type.as_deref(), Some("inode/directory"));
        assert_eq!(
            chooser.selected_application().map(|app| app.id.as_str()),
            None
        );
    }

    #[test]
    fn open_with_chooser_click_selects_row_and_buttons_close_or_commit() {
        let mut scene = test_scene(vec![test_entry("note.txt", false)], ShellViewMode::Icons);
        scene.open_with_chooser = Some(ShellOpenWithChooser::new(
            PathBuf::from("/tmp/note.txt"),
            Some(Arc::from("text/plain")),
            vec![
                MimeApplication {
                    id: "viewer.desktop".to_string(),
                    desktop_file: PathBuf::from("/apps/viewer.desktop"),
                    name: "Viewer".to_string(),
                    exec: "viewer %f".to_string(),
                    icon: None,
                    is_default: false,
                },
                MimeApplication {
                    id: "writer.desktop".to_string(),
                    desktop_file: PathBuf::from("/apps/writer.desktop"),
                    name: "Writer".to_string(),
                    exec: "writer %f".to_string(),
                    icon: None,
                    is_default: false,
                },
            ],
            Vec::new(),
        ));
        let size = PhysicalSize::new(640, 420);
        let rect = open_with_chooser_rect(scene.open_with_chooser.as_ref().unwrap(), size);
        let list = open_with_chooser_list_rect(rect, scene.open_with_chooser.as_ref().unwrap());

        assert_eq!(
            scene.open_with_chooser_click_at_screen_point(
                ViewPoint {
                    x: list.x + 4.0,
                    y: list.y + 4.0,
                },
                size,
            ),
            OpenWithChooserClick::Row(0)
        );
        assert!(scene.select_open_with_filtered_row(0));
        assert!(
            scene
                .open_with_chooser
                .as_ref()
                .unwrap()
                .selected_application()
                .is_none()
        );
        let rect = open_with_chooser_rect(scene.open_with_chooser.as_ref().unwrap(), size);
        let list = open_with_chooser_list_rect(rect, scene.open_with_chooser.as_ref().unwrap());
        assert_eq!(
            scene.open_with_chooser_click_at_screen_point(
                ViewPoint {
                    x: list.x + 4.0,
                    y: list.y + OPEN_WITH_CHOOSER_ROW_HEIGHT * 2.0 + 4.0,
                },
                size,
            ),
            OpenWithChooserClick::Row(2)
        );
        assert!(scene.select_open_with_filtered_row(2));
        assert_eq!(
            scene
                .open_with_chooser
                .as_ref()
                .unwrap()
                .selected_application()
                .map(|application| application.id.as_str()),
            Some("writer.desktop")
        );
        assert_eq!(
            scene.open_with_chooser_click_at_screen_point(
                ViewPoint {
                    x: open_with_chooser_open_button_rect(rect).x + 2.0,
                    y: open_with_chooser_open_button_rect(rect).y + 2.0,
                },
                size,
            ),
            OpenWithChooserClick::Open
        );
        assert_eq!(
            scene.open_with_chooser_click_at_screen_point(
                ViewPoint {
                    x: size.width as f32 + 1.0,
                    y: 1.0,
                },
                size,
            ),
            OpenWithChooserClick::Outside
        );
    }

    #[test]
    fn open_with_chooser_checkbox_toggles_default_setting() {
        let mut scene = test_scene(vec![test_entry("note.txt", false)], ShellViewMode::Icons);
        scene.open_with_chooser = Some(ShellOpenWithChooser::new(
            PathBuf::from("/tmp/note.txt"),
            Some(Arc::from("text/plain")),
            vec![MimeApplication {
                id: "writer.desktop".to_string(),
                desktop_file: PathBuf::from("/apps/writer.desktop"),
                name: "Writer".to_string(),
                exec: "writer %f".to_string(),
                icon: None,
                is_default: false,
            }],
            Vec::new(),
        ));
        let size =
            open_with_chooser_window_size_scaled(scene.open_with_chooser.as_ref().unwrap(), 1.0);
        let rect = open_with_chooser_rect(scene.open_with_chooser.as_ref().unwrap(), size);
        let checkbox = open_with_chooser_default_checkbox_rect(
            rect,
            scene.open_with_chooser.as_ref().unwrap(),
        );

        assert_eq!(
            scene.open_with_chooser_click_at_screen_point(
                ViewPoint {
                    x: checkbox.x + 2.0,
                    y: checkbox.y + 2.0,
                },
                size,
            ),
            OpenWithChooserClick::ToggleDefault
        );
        assert!(scene.toggle_open_with_set_default());
        assert!(scene.open_with_chooser.as_ref().unwrap().set_as_default);
    }

    #[test]
    fn open_with_chooser_query_cursor_edits_and_uses_text_cursor() {
        let mut scene = test_scene(vec![test_entry("note.txt", false)], ShellViewMode::Icons);
        scene.open_with_chooser = Some(ShellOpenWithChooser::new(
            PathBuf::from("/tmp/note.txt"),
            Some(Arc::from("text/plain")),
            vec![MimeApplication {
                id: "paint.desktop".to_string(),
                desktop_file: PathBuf::from("/apps/paint.desktop"),
                name: "Paint".to_string(),
                exec: "paint %f".to_string(),
                icon: None,
                is_default: false,
            }],
            Vec::new(),
        ));
        let size = PhysicalSize::new(640, 460);

        assert!(scene.apply_open_with_command(OpenWithCommand::Insert("paβnt".to_string())));
        assert_eq!(scene.open_with_chooser.as_ref().unwrap().query, "paβnt");
        assert_eq!(
            scene.open_with_chooser.as_ref().unwrap().query_cursor,
            "paβnt".len()
        );

        assert!(scene.apply_open_with_command(OpenWithCommand::MoveLeft));
        assert!(scene.apply_open_with_command(OpenWithCommand::MoveLeft));
        assert!(scene.apply_open_with_command(OpenWithCommand::Backspace));
        assert_eq!(scene.open_with_chooser.as_ref().unwrap().query, "pant");
        assert_eq!(scene.open_with_chooser.as_ref().unwrap().query_cursor, 2);

        assert!(scene.apply_open_with_command(OpenWithCommand::Insert("i".to_string())));
        assert_eq!(scene.open_with_chooser.as_ref().unwrap().query, "paint");
        assert_eq!(scene.open_with_chooser.as_ref().unwrap().query_cursor, 3);

        assert!(scene.apply_open_with_command(OpenWithCommand::MoveEnd));
        assert!(scene.apply_open_with_command(OpenWithCommand::MoveLeft));
        assert!(scene.apply_open_with_command(OpenWithCommand::Delete));
        assert_eq!(scene.open_with_chooser.as_ref().unwrap().query, "pain");
        assert_eq!(scene.open_with_chooser.as_ref().unwrap().query_cursor, 4);

        let rect = open_with_chooser_rect(scene.open_with_chooser.as_ref().unwrap(), size);
        let query = open_with_chooser_query_rect_scaled(rect, 1.0);
        scene.set_pointer(
            ViewPoint {
                x: query.x + 4.0,
                y: query.y + 4.0,
            },
            size,
        );
        assert_eq!(scene.open_with_chooser_cursor_icon(size), CursorIcon::Text);
        assert_eq!(
            scene.open_with_chooser_click_at_screen_point(
                ViewPoint {
                    x: query.right() - 2.0,
                    y: query.y + query.height / 2.0,
                },
                size,
            ),
            OpenWithChooserClick::Query("pain".len())
        );
        let query_text = open_with_chooser_query_text_rect_scaled(rect, 1.0);
        let query_mid = estimated_text_cursor_x("pain", "pa".len(), TEXT_FONT_SIZE);
        assert_eq!(
            scene.open_with_chooser_click_at_screen_point(
                ViewPoint {
                    x: query_text.x + query_mid,
                    y: query.y + query.height / 2.0,
                },
                size,
            ),
            OpenWithChooserClick::Query("pa".len())
        );
        assert!(scene.set_open_with_query_cursor("pa".len()));
        assert_eq!(
            scene.open_with_chooser.as_ref().unwrap().query_cursor,
            "pa".len()
        );
        let query_tail = estimated_text_cursor_x("pain", "pain".len(), TEXT_FONT_SIZE);
        assert_eq!(
            scene.open_with_chooser_click_at_screen_point(
                ViewPoint {
                    x: query_text.x + query_tail + 12.0,
                    y: query.y + query.height / 2.0,
                },
                size,
            ),
            OpenWithChooserClick::Query("pain".len())
        );
        let list = open_with_chooser_list_rect(rect, scene.open_with_chooser.as_ref().unwrap());
        scene.set_pointer(
            ViewPoint {
                x: list.x + 4.0,
                y: list.y + 4.0,
            },
            size,
        );
        assert_eq!(
            scene.open_with_chooser_cursor_icon(size),
            CursorIcon::Pointer
        );
        assert!(scene.set_open_with_query_cursor(0));
        assert_eq!(scene.open_with_chooser.as_ref().unwrap().query_cursor, 0);
    }

    #[test]
    fn open_with_chooser_scrolls_visible_applications_without_changing_selection() {
        let mut scene = test_scene(vec![test_entry("note.txt", false)], ShellViewMode::Icons);
        let applications = (0..12)
            .map(|index| MimeApplication {
                id: format!("app{index}.desktop"),
                desktop_file: PathBuf::from(format!("/apps/app{index}.desktop")),
                name: format!("App {index}"),
                exec: format!("app{index} %f"),
                icon: None,
                is_default: index == 0,
            })
            .collect::<Vec<_>>();
        scene.open_with_chooser = Some(ShellOpenWithChooser::new(
            PathBuf::from("/tmp/note.txt"),
            Some(Arc::from("text/plain")),
            applications,
            Vec::new(),
        ));
        assert!(scene.select_open_with_filtered_row(0));

        assert!(scene.scroll_open_with_chooser_by(OPEN_WITH_CHOOSER_ROW_HEIGHT));
        let chooser = scene.open_with_chooser.as_ref().unwrap();
        assert_eq!(chooser.scroll_row, 1);
        assert_eq!(chooser.selected_index, 0);
        assert!(matches!(
            chooser.visible_tree_rows().first(),
            Some(OpenWithTreeRow::Application { app_index: 0 })
        ));

        assert!(scene.scroll_open_with_chooser_by(OPEN_WITH_CHOOSER_ROW_HEIGHT * 99.0));
        assert_eq!(scene.open_with_chooser.as_ref().unwrap().scroll_row, 5);

        assert!(scene.scroll_open_with_chooser_by(-OPEN_WITH_CHOOSER_ROW_HEIGHT));
        assert_eq!(scene.open_with_chooser.as_ref().unwrap().scroll_row, 4);

        assert!(scene.scroll_open_with_chooser_by(-OPEN_WITH_CHOOSER_ROW_HEIGHT * 99.0));
        assert_eq!(scene.open_with_chooser.as_ref().unwrap().scroll_row, 0);
    }

    #[test]
    fn open_with_chooser_scrollbar_thumb_drag_updates_visible_rows() {
        let mut scene = test_scene(vec![test_entry("note.txt", false)], ShellViewMode::Icons);
        let applications = (0..12)
            .map(|index| MimeApplication {
                id: format!("app{index}.desktop"),
                desktop_file: PathBuf::from(format!("/apps/app{index}.desktop")),
                name: format!("App {index}"),
                exec: format!("app{index} %f"),
                icon: None,
                is_default: index == 0,
            })
            .collect::<Vec<_>>();
        scene.open_with_chooser = Some(ShellOpenWithChooser::new(
            PathBuf::from("/tmp/note.txt"),
            Some(Arc::from("text/plain")),
            applications,
            Vec::new(),
        ));
        assert!(scene.select_open_with_filtered_row(0));
        let size = PhysicalSize::new(700, 560);
        let (track, thumb) = scene
            .open_with_chooser_scrollbar_rects(size)
            .expect("overflowing open-with chooser should show a scrollbar");
        let press = ViewPoint {
            x: thumb.x + thumb.width / 2.0,
            y: thumb.y + thumb.height / 2.0,
        };
        let drag_to = ViewPoint {
            x: press.x,
            y: track.bottom() - thumb.height / 2.0,
        };

        assert_eq!(
            scene.begin_open_with_scrollbar_drag(press, size),
            Some(false)
        );
        assert_eq!(
            scene.scrollbar_drag.map(|drag| drag.target),
            Some(ScrollbarDragTarget::OpenWith)
        );
        assert!(scene.set_pointer(drag_to, size));
        assert_eq!(scene.open_with_chooser.as_ref().unwrap().scroll_row, 5);
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].scroll_y, 0.0);

        let _ = scene.end_scrollbar_drag(drag_to, size);
        assert!(scene.scrollbar_drag.is_none());
    }
