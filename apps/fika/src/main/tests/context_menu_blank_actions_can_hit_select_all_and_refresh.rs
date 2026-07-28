
    #[test]
    fn context_menu_blank_actions_can_hit_select_all_and_refresh() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(420, 420);
        let content = scene
            .pane_geometry(ShellPaneId::SLOT_0, size)
            .unwrap()
            .content;
        let point = ViewPoint {
            x: content.right() - 4.0,
            y: scene.content_origin_y() + 4.0,
        };

        assert!(scene.open_context_menu(point, size));
        let rect = context_menu_rect(scene.context_menu.as_ref().unwrap(), size);
        let actions = context_menu_actions(&scene.context_menu.as_ref().unwrap().target);
        let select_all_index = actions
            .iter()
            .position(|action| *action == ShellContextMenuAction::SelectAll)
            .unwrap();
        let select_all_row = ViewPoint {
            x: rect.x + 8.0,
            y: rect.y
                + CONTEXT_MENU_VERTICAL_PADDING
                + CONTEXT_MENU_ROW_HEIGHT * select_all_index as f32
                + 8.0,
        };
        assert_eq!(
            scene.activate_or_close_context_menu(select_all_row, size),
            Some(ShellContextMenuAction::SelectAll)
        );
        assert_eq!(scene.context_menu_actions, 1);

        assert!(scene.open_context_menu(point, size));
        let rect = context_menu_rect(scene.context_menu.as_ref().unwrap(), size);
        let actions = context_menu_actions(&scene.context_menu.as_ref().unwrap().target);
        let refresh_index = actions
            .iter()
            .position(|action| *action == ShellContextMenuAction::Refresh)
            .unwrap();
        let refresh_row = ViewPoint {
            x: rect.x + 8.0,
            y: rect.y
                + CONTEXT_MENU_VERTICAL_PADDING
                + CONTEXT_MENU_ROW_HEIGHT * refresh_index as f32
                + 8.0,
        };
        assert_eq!(
            scene.activate_or_close_context_menu(refresh_row, size),
            Some(ShellContextMenuAction::Refresh)
        );
        assert_eq!(scene.context_menu_actions, 2);
    }

    #[test]
    fn context_menu_exposes_explicit_administrator_actions() {
        let blank = ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_0,
            path: PathBuf::from("/tmp"),
        };
        let blank_actions = context_menu_actions(&blank);
        assert!(blank_actions.contains(&ShellContextMenuAction::Paste));
        assert!(blank_actions.contains(&ShellContextMenuAction::PasteAsAdministrator));

        let item = ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: PathBuf::from("/tmp/file.txt"),
            is_dir: false,
            selection_count: 1,
        };
        let item_actions = context_menu_actions(&item);
        assert!(item_actions.contains(&ShellContextMenuAction::Rename));
        assert!(item_actions.contains(&ShellContextMenuAction::RenameAsAdministrator));
        assert!(item_actions.contains(&ShellContextMenuAction::MoveToTrash));
        assert!(item_actions.contains(&ShellContextMenuAction::MoveToTrashAsAdministrator));

        let menu = ShellContextMenu::new(blank, ViewPoint { x: 0.0, y: 0.0 });
        let create_items = context_submenu_actions(ShellContextSubmenu::CreateNew, &menu);
        assert!(create_items.iter().any(|item| {
            matches!(
                item.command,
                ShellContextMenuCommand::CreateEntry {
                    kind: CreateEntryKind::Folder,
                    privileged: true
                }
            )
        }));
        assert!(create_items.iter().any(|item| {
            matches!(
                item.command,
                ShellContextMenuCommand::CreateEntry {
                    kind: CreateEntryKind::File,
                    privileged: true
                }
            )
        }));
    }

    #[test]
    fn context_menu_blank_target_exposes_view_mode_submenu() {
        let blank = ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_0,
            path: PathBuf::from("/tmp"),
        };
        let menu = ShellContextMenu::new(blank.clone(), ViewPoint { x: 0.0, y: 0.0 });
        let items = context_menu_items(&menu);
        assert!(items.iter().any(|item| {
            item.submenu == Some(ShellContextSubmenu::ViewMode)
                && matches!(
                    item.command,
                    ShellContextMenuCommand::OpenSubmenu(ShellContextSubmenu::ViewMode)
                )
        }));

        let modes = context_submenu_actions(ShellContextSubmenu::ViewMode, &menu);
        assert_eq!(modes.len(), 3);
        assert!(matches!(
            modes[0].command,
            ShellContextMenuCommand::SetViewMode(ShellViewMode::Icons)
        ));
        assert!(matches!(
            modes[1].command,
            ShellContextMenuCommand::SetViewMode(ShellViewMode::Compact)
        ));
        assert!(matches!(
            modes[2].command,
            ShellContextMenuCommand::SetViewMode(ShellViewMode::Details)
        ));

        let actions = context_menu_actions(&blank);
        let view_mode_row = actions
            .iter()
            .position(|action| *action == ShellContextMenuAction::ViewMode)
            .unwrap();
        assert!(context_menu_separator_before(&blank, view_mode_row));
    }

    #[test]
    fn default_open_file_launch_request_uses_mime_default_application_plan() {
        let cache = MimeApplicationCache::from_applications_and_mimeapps(
            vec![
                test_desktop_application("viewer.desktop", "Viewer", "viewer %f", &["text/plain"]),
                test_desktop_application("other.desktop", "Other", "other %f", &["text/plain"]),
            ],
            &[fika_core::MimeAppsList {
                default_apps: HashMap::from([(
                    "text/plain".to_string(),
                    vec!["viewer.desktop".to_string()],
                )]),
                ..Default::default()
            }],
        );
        let request = OpenFileRequest {
            path: PathBuf::from("/tmp/plain.txt"),
            uri: "file:///tmp/plain.txt".to_string(),
            mime_type: Some("text/plain".to_string()),
        };

        let launch = default_open_file_launch_request(&cache, &request).unwrap();

        assert_eq!(launch.path, PathBuf::from("/tmp/plain.txt"));
        assert_eq!(launch.app_name, "Viewer");
        assert_eq!(launch.plan.commands[0].program, "viewer");
        assert_eq!(launch.plan.commands[0].args, vec!["/tmp/plain.txt"]);
    }

    #[test]
    fn copy_location_request_uses_target_display_path() {
        let mut scene = test_scene(vec![test_entry("plain.txt", false)], ShellViewMode::Icons);
        scene.context_target = Some(ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_0,
            path: PathBuf::from("/tmp"),
        });
        assert_eq!(scene.context_target_copy_location_request(), None);

        scene.context_target = Some(ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: PathBuf::from("/tmp/Fika Test/plain.txt"),
            is_dir: false,
            selection_count: 1,
        });
        let request = scene
            .context_target_copy_location_request()
            .expect("item target should produce copy location request");
        assert_eq!(
            request,
            CopyLocationRequest {
                path: PathBuf::from("/tmp/Fika Test/plain.txt"),
                text: "/tmp/Fika Test/plain.txt".to_string(),
            }
        );

        scene.record_copy_location(&request);
        assert_eq!(scene.copy_location_changes, 1);

        scene.context_target = Some(ShellContextTarget::Place {
            index: 1,
            label: "Root".to_string(),
            path: PathBuf::from("/"),
            group: "Devices",
            device: None,
            network: false,
            trash: false,
            root: true,
            editable: false,
        });
        assert_eq!(
            scene.context_target_copy_location_request(),
            Some(CopyLocationRequest {
                path: PathBuf::from("/"),
                text: "/".to_string(),
            })
        );
    }

    #[test]
    fn file_clipboard_request_uses_multi_selection_and_rejects_remote_cut() {
        let mut scene = test_scene(
            vec![
                test_entry("one.txt", false),
                test_entry("two.txt", false),
                test_entry("remote.txt", false),
            ],
            ShellViewMode::Icons,
        );
        scene.context_target = Some(ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 1,
            path: PathBuf::from("/tmp/two.txt"),
            is_dir: false,
            selection_count: 2,
        });
        scene.panes[ShellPaneId::SLOT_0]
            .selection
            .select_indexes(&[0, 1]);

        let request = scene
            .context_target_file_clipboard_request(ShellContextMenuAction::Copy)
            .unwrap()
            .expect("selected item target should produce clipboard request");
        assert_eq!(request.role, FileClipboardRole::Copy);
        assert_eq!(
            request.paths,
            vec![PathBuf::from("/tmp/one.txt"), PathBuf::from("/tmp/two.txt")]
        );
        assert_eq!(
            request.text,
            encode_file_clipboard_text(FileClipboardRole::Copy, &request.paths)
        );

        scene.record_file_clipboard_export(&request);
        assert_eq!(scene.file_clipboard_changes, 1);

        scene.context_target = Some(ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 2,
            path: PathBuf::from("sftp://example.test/home/yk/remote.txt"),
            is_dir: false,
            selection_count: 1,
        });
        assert!(
            scene
                .context_target_file_clipboard_request(ShellContextMenuAction::Cut)
                .unwrap_err()
                .contains("remote cut")
        );
    }

    #[test]
    fn paste_file_clipboard_copies_files_reloads_and_keeps_clipboard() {
        let source_root = test_dir("paste-file-source");
        let target_root = test_dir("paste-file-target");
        fs::create_dir_all(&source_root).unwrap();
        fs::create_dir_all(&target_root).unwrap();
        fs::write(source_root.join("source.txt"), b"source").unwrap();
        let size = PhysicalSize::new(420, 260);
        let mut scene = ShellScene::load(target_root.clone(), ShellViewMode::Icons).unwrap();
        scene.context_target = Some(ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_0,
            path: target_root.clone(),
        });
        let clipboard_text =
            encode_file_clipboard_text(FileClipboardRole::Copy, &[source_root.join("source.txt")]);

        let result = scene
            .paste_clipboard_text_from_context(&clipboard_text, size, false)
            .unwrap();

        assert_eq!(result.mode, FileTransferMode::Copy);
        assert_eq!(result.success_count, 1);
        assert_eq!(result.failure_count, 0);
        assert!(!result.clear_clipboard);
        assert_eq!(scene.paste_changes, 1);
        assert_eq!(scene.directory_reloads, 1);
        assert!(target_root.join("source.txt").is_file());
        assert_eq!(fs::read(target_root.join("source.txt")).unwrap(), b"source");
        assert!(
            entry_index_by_name(&scene.panes[ShellPaneId::SLOT_0].entries, "source.txt").is_some()
        );

        fs::remove_dir_all(source_root).unwrap();
        fs::remove_dir_all(target_root).unwrap();
    }

    #[test]
    fn paste_cut_file_moves_file_and_requests_clipboard_clear() {
        let source_root = test_dir("paste-cut-source");
        let target_root = test_dir("paste-cut-target");
        fs::create_dir_all(&source_root).unwrap();
        fs::create_dir_all(&target_root).unwrap();
        fs::write(source_root.join("move.txt"), b"move").unwrap();
        let size = PhysicalSize::new(420, 260);
        let mut scene = ShellScene::load(target_root.clone(), ShellViewMode::Icons).unwrap();
        scene.context_target = Some(ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_0,
            path: target_root.clone(),
        });
        let clipboard_text =
            encode_file_clipboard_text(FileClipboardRole::Cut, &[source_root.join("move.txt")]);

        let result = scene
            .paste_clipboard_text_from_context(&clipboard_text, size, false)
            .unwrap();

        assert_eq!(result.mode, FileTransferMode::Move);
        assert_eq!(result.success_count, 1);
        assert_eq!(result.failure_count, 0);
        assert!(result.clear_clipboard);
        assert!(!source_root.join("move.txt").exists());
        assert!(target_root.join("move.txt").is_file());
        assert_eq!(scene.paste_changes, 1);
        assert_eq!(scene.directory_reloads, 1);

        fs::remove_dir_all(source_root).unwrap();
        fs::remove_dir_all(target_root).unwrap();
    }

    #[test]
    fn paste_plain_text_creates_unique_text_file() {
        let root = test_dir("paste-text");
        fs::create_dir_all(&root).unwrap();
        let size = PhysicalSize::new(420, 260);
        let mut scene = ShellScene::load(root.clone(), ShellViewMode::Icons).unwrap();
        scene.context_target = Some(ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_0,
            path: root.clone(),
        });

        let result = scene
            .paste_clipboard_text_from_context("hello from clipboard", size, false)
            .unwrap();

        assert_eq!(result.mode, FileTransferMode::Copy);
        assert_eq!(result.success_count, 1);
        assert_eq!(result.failure_count, 0);
        assert!(!result.clear_clipboard);
        assert_eq!(scene.paste_changes, 1);
        assert_eq!(scene.directory_reloads, 1);
        assert_eq!(
            fs::read_to_string(root.join("Pasted Text.txt")).unwrap(),
            "hello from clipboard"
        );
        assert!(
            entry_index_by_name(&scene.panes[ShellPaneId::SLOT_0].entries, "Pasted Text.txt")
                .is_some()
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn properties_overlay_builds_item_metadata_from_context_target() {
        let mut scene = test_scene(
            vec![test_entry("folder", true), test_entry("plain.txt", false)],
            ShellViewMode::Icons,
        );
        scene.context_target = Some(ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 1,
            path: PathBuf::from("/tmp/plain.txt"),
            is_dir: false,
            selection_count: 2,
        });

        assert!(scene.open_properties_overlay_from_context());
        let overlay = scene
            .properties_overlay
            .as_ref()
            .expect("properties overlay should open");
        assert_eq!(overlay.title, "Properties - plain.txt");
        assert!(
            overlay
                .rows
                .iter()
                .any(|row| row.label == "Name" && row.value == "plain.txt")
        );
        assert!(
            overlay
                .rows
                .iter()
                .any(|row| row.label == "Type" && row.value == "File")
        );
        assert!(
            overlay
                .rows
                .iter()
                .any(|row| row.label == "Selection" && row.value == "2 items")
        );
        assert!(
            overlay
                .rows
                .iter()
                .any(|row| row.label == "Path" && row.value == "/tmp/plain.txt")
        );
        assert_eq!(scene.properties_changes, 1);
    }

    #[test]
    fn properties_overlay_builds_blank_directory_summary_and_closes_outside() {
        let mut scene = test_scene(
            vec![test_entry("folder", true), test_entry("plain.txt", false)],
            ShellViewMode::Icons,
        );
        scene.context_target = Some(ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_0,
            path: PathBuf::from("/tmp"),
        });
        let size = PhysicalSize::new(360, 240);

        assert!(scene.open_properties_overlay_from_context());
        let overlay = scene
            .properties_overlay
            .as_ref()
            .expect("properties overlay should open");
        assert_eq!(overlay.title, "Properties - /tmp");
        assert!(
            overlay
                .rows
                .iter()
                .any(|row| row.label == "Entries" && row.value == "2")
        );
        assert!(
            overlay
                .rows
                .iter()
                .any(|row| row.label == "Folders" && row.value == "1")
        );
        assert!(
            overlay
                .rows
                .iter()
                .any(|row| row.label == "Files" && row.value == "1")
        );
        let rect = properties_overlay_rect(overlay, size);
        assert!(rect.x >= PROPERTIES_OVERLAY_MARGIN);
        assert!(rect.y >= PROPERTIES_OVERLAY_MARGIN);
        assert!(!scene.close_properties_overlay_if_outside(
            ViewPoint {
                x: rect.x + 2.0,
                y: rect.y + 2.0,
            },
            size,
        ));
        assert!(scene.close_properties_overlay_if_outside(ViewPoint { x: 1.0, y: 1.0 }, size,));
        assert!(scene.properties_overlay.is_none());
        assert_eq!(scene.properties_changes, 2);
    }
