
    #[test]
    fn path_history_tracks_back_forward_and_clears_forward_on_new_navigation() {
        let unique = format!(
            "fika-wgpu-history-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = env::temp_dir().join(unique);
        let first = root.join("first");
        let second = first.join("second");
        let sibling = root.join("sibling");
        fs::create_dir_all(&second).unwrap();
        fs::create_dir_all(&sibling).unwrap();

        let size = PhysicalSize::new(360, 240);
        let mut scene = ShellScene::load(root.clone(), ShellViewMode::Compact).unwrap();

        assert!(scene.load_path(first.clone(), size).unwrap());
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].path, first);
        assert_eq!(
            scene.pane_history(ShellPaneId::SLOT_0).back,
            vec![root.clone()]
        );
        assert!(scene.pane_history(ShellPaneId::SLOT_0).forward.is_empty());

        assert!(scene.load_path(second.clone(), size).unwrap());
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].path, second);
        assert_eq!(
            scene.pane_history(ShellPaneId::SLOT_0).back,
            vec![root.clone(), first.clone()]
        );

        assert!(scene.go_history_back(size).unwrap());
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].path, first);
        assert_eq!(
            scene.pane_history(ShellPaneId::SLOT_0).back,
            vec![root.clone()]
        );
        assert_eq!(
            scene.pane_history(ShellPaneId::SLOT_0).forward,
            vec![second.clone()]
        );

        assert!(scene.go_history_forward(size).unwrap());
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].path, second);
        assert_eq!(
            scene.pane_history(ShellPaneId::SLOT_0).back,
            vec![root.clone(), first.clone()]
        );
        assert!(scene.pane_history(ShellPaneId::SLOT_0).forward.is_empty());

        assert!(scene.go_history_back(size).unwrap());
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].path, first);
        assert!(scene.load_path(sibling.clone(), size).unwrap());
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].path, sibling);
        assert!(scene.pane_history(ShellPaneId::SLOT_0).forward.is_empty());
        assert_eq!(
            scene.pane_history(ShellPaneId::SLOT_0).back,
            vec![root.clone(), first]
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_path_load_keeps_the_current_directory() {
        let root = test_dir("path-load-failure");
        fs::create_dir_all(&root).unwrap();
        let size = PhysicalSize::new(360, 240);
        let mut scene = ShellScene::load(root.clone(), ShellViewMode::Compact).unwrap();

        assert!(scene.load_path(root.join("missing"), size).is_err());
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].path, root);

        fs::remove_dir_all(scene.panes[ShellPaneId::SLOT_0].path.clone()).unwrap();
    }

    #[test]
    fn reload_current_path_preserves_history_and_selection_by_name() {
        let unique = format!(
            "fika-wgpu-reload-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = env::temp_dir().join(unique);
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("keep.txt"), b"keep").unwrap();

        let size = PhysicalSize::new(360, 240);
        let mut scene = ShellScene::load(root.clone(), ShellViewMode::Compact).unwrap();
        let keep_index =
            entry_index_by_name(&scene.panes[ShellPaneId::SLOT_0].entries, "keep.txt").unwrap();
        assert!(
            scene.panes[ShellPaneId::SLOT_0]
                .selection
                .apply_navigation(keep_index, false)
        );
        scene
            .pane_history_mut(ShellPaneId::SLOT_0)
            .push_back(PathBuf::from("/tmp/previous"));
        scene
            .pane_history_mut(ShellPaneId::SLOT_0)
            .push_forward(PathBuf::from("/tmp/next"));

        fs::write(root.join("aaa.txt"), b"new").unwrap();
        assert!(scene.reload_current_path(size).unwrap());

        let new_keep_index =
            entry_index_by_name(&scene.panes[ShellPaneId::SLOT_0].entries, "keep.txt").unwrap();
        assert!(
            scene.panes[ShellPaneId::SLOT_0]
                .selection
                .contains(new_keep_index)
        );
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].selection.len(), 1);
        assert_eq!(
            scene.panes[ShellPaneId::SLOT_0].selection.focus,
            Some(new_keep_index)
        );
        assert_eq!(
            scene.pane_history(ShellPaneId::SLOT_0).back,
            vec![PathBuf::from("/tmp/previous")]
        );
        assert_eq!(
            scene.pane_history(ShellPaneId::SLOT_0).forward,
            vec![PathBuf::from("/tmp/next")]
        );
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].path, root);
        assert_eq!(scene.path_changes, 0);
        assert_eq!(scene.directory_reloads, 1);

        fs::remove_dir_all(scene.panes[ShellPaneId::SLOT_0].path.clone()).unwrap();
    }

    #[test]
    fn screen_to_content_point_rejects_top_bar() {
        let offset = ViewPoint { x: 0.0, y: 40.0 };
        let content_rect = ViewRect {
            x: 180.0,
            y: TOP_BAR_HEIGHT,
            width: 320.0,
            height: 160.0,
        };
        assert_eq!(
            screen_to_content_point(ViewPoint { x: 190.0, y: 10.0 }, offset, content_rect),
            None
        );
        assert_eq!(
            screen_to_content_point(
                ViewPoint {
                    x: content_rect.x - 1.0,
                    y: TOP_BAR_HEIGHT + 5.0
                },
                offset,
                content_rect
            ),
            None
        );
        assert_eq!(
            screen_to_content_point(
                ViewPoint {
                    x: 192.0,
                    y: TOP_BAR_HEIGHT + 5.0
                },
                offset,
                content_rect
            ),
            Some(ViewPoint { x: 12.0, y: 45.0 })
        );
    }

    #[test]
    fn view_mode_shortcuts_accept_ctrl_digits_and_function_key_fallbacks() {
        let unidentified = PhysicalKey::Unidentified(NativeKeyCode::Unidentified);
        let no_key = Key::Unidentified(NativeKey::Unidentified);

        assert_eq!(
            view_mode_for_key_parts(true, &PhysicalKey::Code(KeyCode::Digit1), &no_key, &no_key,),
            Some(ShellViewMode::Icons)
        );
        assert_eq!(
            view_mode_for_key_parts(true, &PhysicalKey::Code(KeyCode::Numpad2), &no_key, &no_key,),
            Some(ShellViewMode::Compact)
        );
        assert_eq!(
            view_mode_for_key_parts(true, &unidentified, &Key::Character("3".into()), &no_key,),
            Some(ShellViewMode::Details)
        );
        assert_eq!(
            view_mode_for_key_parts(
                true,
                &unidentified,
                &Key::Character("!".into()),
                &Key::Character("1".into()),
            ),
            Some(ShellViewMode::Icons)
        );
        assert_eq!(
            view_mode_for_key_parts(false, &PhysicalKey::Code(KeyCode::F3), &no_key, &no_key,),
            Some(ShellViewMode::Details)
        );
        assert_eq!(
            view_mode_for_key_parts(
                false,
                &PhysicalKey::Code(KeyCode::Digit1),
                &Key::Character("1".into()),
                &Key::Character("1".into()),
            ),
            None
        );
        assert_eq!(
            view_mode_for_key_parts(false, &PhysicalKey::Code(KeyCode::F2), &no_key, &no_key,),
            None
        );
    }

    #[test]
    fn file_keyboard_shortcuts_cover_clipboard_rename_and_delete() {
        let no_key = Key::Unidentified(NativeKey::Unidentified);

        assert_eq!(
            file_keyboard_command_for_key_parts(
                false,
                &PhysicalKey::Code(KeyCode::F2),
                &no_key,
                &no_key
            ),
            Some(FileKeyboardCommand::Rename)
        );
        assert_eq!(
            file_keyboard_command_for_key_parts(
                false,
                &PhysicalKey::Code(KeyCode::Delete),
                &no_key,
                &no_key
            ),
            Some(FileKeyboardCommand::Delete)
        );
        assert_eq!(
            file_keyboard_command_for_key_parts(
                true,
                &PhysicalKey::Code(KeyCode::KeyC),
                &no_key,
                &no_key
            ),
            Some(FileKeyboardCommand::Copy)
        );
        assert_eq!(
            file_keyboard_command_for_key_parts(
                true,
                &PhysicalKey::Code(KeyCode::KeyX),
                &no_key,
                &no_key
            ),
            Some(FileKeyboardCommand::Cut)
        );
        assert_eq!(
            file_keyboard_command_for_key_parts(
                true,
                &PhysicalKey::Code(KeyCode::KeyV),
                &no_key,
                &no_key
            ),
            Some(FileKeyboardCommand::Paste)
        );
        assert_eq!(
            file_keyboard_command_for_key_parts(
                false,
                &PhysicalKey::Code(KeyCode::KeyC),
                &Key::Character("c".into()),
                &Key::Character("c".into())
            ),
            None
        );
    }

    #[test]
    fn path_navigation_shortcuts_use_back_forward_and_parent_keys() {
        assert_eq!(
            path_navigation_action_for_key(&Key::Named(NamedKey::Backspace), false),
            Some(PathNavigationAction::Parent)
        );
        assert_eq!(
            path_navigation_action_for_key(&Key::Named(NamedKey::ArrowLeft), true),
            Some(PathNavigationAction::Back)
        );
        assert_eq!(
            path_navigation_action_for_key(&Key::Named(NamedKey::ArrowRight), true),
            Some(PathNavigationAction::Forward)
        );
        assert_eq!(
            path_navigation_action_for_key(&Key::Named(NamedKey::ArrowUp), true),
            Some(PathNavigationAction::Parent)
        );
        assert_eq!(
            path_navigation_action_for_key(&Key::Named(NamedKey::ArrowLeft), false),
            None
        );
        assert_eq!(
            path_navigation_action_for_mouse_button(MouseButton::Back),
            Some(PathNavigationAction::Back)
        );
        assert_eq!(
            path_navigation_action_for_mouse_button(MouseButton::Forward),
            Some(PathNavigationAction::Forward)
        );
        assert_eq!(
            path_navigation_action_for_mouse_button(MouseButton::Left),
            None
        );
    }

    #[test]
    fn reload_shortcuts_accept_f5_and_ctrl_r() {
        let unidentified = PhysicalKey::Unidentified(NativeKeyCode::Unidentified);
        let no_key = Key::Unidentified(NativeKey::Unidentified);

        assert!(reload_requested_for_key_parts(
            false,
            &PhysicalKey::Code(KeyCode::F5),
            &no_key,
            &no_key,
        ));
        assert!(reload_requested_for_key_parts(
            true,
            &PhysicalKey::Code(KeyCode::KeyR),
            &no_key,
            &no_key,
        ));
        assert!(reload_requested_for_key_parts(
            true,
            &unidentified,
            &Key::Character("R".into()),
            &no_key,
        ));
        assert!(!reload_requested_for_key_parts(
            false,
            &PhysicalKey::Code(KeyCode::KeyR),
            &Key::Character("r".into()),
            &Key::Character("r".into()),
        ));
    }

    #[test]
    fn hidden_shortcut_requires_ctrl_h() {
        let unidentified = PhysicalKey::Unidentified(NativeKeyCode::Unidentified);
        let no_key = Key::Unidentified(NativeKey::Unidentified);

        assert!(hidden_toggle_requested_for_key_parts(
            true,
            &PhysicalKey::Code(KeyCode::KeyH),
            &no_key,
            &no_key,
        ));
        assert!(hidden_toggle_requested_for_key_parts(
            true,
            &unidentified,
            &Key::Character("H".into()),
            &no_key,
        ));
        assert!(!hidden_toggle_requested_for_key_parts(
            false,
            &PhysicalKey::Code(KeyCode::KeyH),
            &Key::Character("h".into()),
            &Key::Character("h".into()),
        ));
    }

    #[test]
    fn dark_mode_shortcut_requires_ctrl_shift_d() {
        let unidentified = PhysicalKey::Unidentified(NativeKeyCode::Unidentified);
        let no_key = Key::Unidentified(NativeKey::Unidentified);

        assert!(dark_mode_toggle_requested_for_key_parts(
            true,
            true,
            &PhysicalKey::Code(KeyCode::KeyD),
            &no_key,
            &no_key,
        ));
        assert!(dark_mode_toggle_requested_for_key_parts(
            true,
            true,
            &unidentified,
            &Key::Character("D".into()),
            &no_key,
        ));
        assert!(!dark_mode_toggle_requested_for_key_parts(
            true,
            false,
            &PhysicalKey::Code(KeyCode::KeyD),
            &Key::Character("d".into()),
            &Key::Character("d".into()),
        ));
    }

    #[test]
    fn location_shortcuts_activate_and_capture_text_when_active() {
        let unidentified = PhysicalKey::Unidentified(NativeKeyCode::Unidentified);
        let no_key = Key::Unidentified(NativeKey::Unidentified);

        assert_eq!(
            location_command_for_key_parts(
                true,
                false,
                &PhysicalKey::Code(KeyCode::KeyL),
                &no_key,
                &no_key,
            ),
            Some(LocationCommand::Activate)
        );
        assert_eq!(
            location_command_for_key_parts(
                true,
                false,
                &PhysicalKey::Code(KeyCode::KeyD),
                &no_key,
                &no_key,
            ),
            Some(LocationCommand::Activate)
        );
        assert_eq!(
            location_command_for_key_parts(
                false,
                false,
                &PhysicalKey::Code(KeyCode::F6),
                &no_key,
                &no_key,
            ),
            Some(LocationCommand::Activate)
        );
        assert_eq!(
            location_command_for_key_parts(
                false,
                true,
                &unidentified,
                &Key::Character("/".into()),
                &no_key,
            ),
            Some(LocationCommand::Insert("/".to_string()))
        );
        assert_eq!(
            location_command_for_key_parts(
                false,
                true,
                &PhysicalKey::Code(KeyCode::Tab),
                &no_key,
                &no_key,
            ),
            Some(LocationCommand::Complete)
        );
        assert_eq!(
            location_command_for_key_parts(
                false,
                true,
                &PhysicalKey::Code(KeyCode::Delete),
                &no_key,
                &no_key,
            ),
            Some(LocationCommand::Delete)
        );
        assert_eq!(
            location_command_for_key_parts(
                false,
                true,
                &PhysicalKey::Code(KeyCode::ArrowLeft),
                &no_key,
                &no_key,
            ),
            Some(LocationCommand::MoveLeft)
        );
        assert_eq!(
            location_command_for_key_parts(
                false,
                true,
                &PhysicalKey::Code(KeyCode::ArrowRight),
                &no_key,
                &no_key,
            ),
            Some(LocationCommand::MoveRight)
        );
        assert_eq!(
            location_command_for_key_parts(
                false,
                true,
                &PhysicalKey::Code(KeyCode::Home),
                &no_key,
                &no_key,
            ),
            Some(LocationCommand::MoveHome)
        );
        assert_eq!(
            location_command_for_key_parts(
                false,
                true,
                &PhysicalKey::Code(KeyCode::End),
                &no_key,
                &no_key,
            ),
            Some(LocationCommand::MoveEnd)
        );
        assert_eq!(
            location_command_for_key_parts(
                false,
                true,
                &PhysicalKey::Code(KeyCode::Escape),
                &no_key,
                &no_key,
            ),
            Some(LocationCommand::Cancel)
        );
        assert_eq!(
            location_command_for_key_parts(
                true,
                true,
                &PhysicalKey::Code(KeyCode::KeyR),
                &Key::Character("r".into()),
                &Key::Character("r".into()),
            ),
            Some(LocationCommand::Ignore)
        );
    }
