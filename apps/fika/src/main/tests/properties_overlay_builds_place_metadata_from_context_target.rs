
    #[test]
    fn properties_overlay_builds_place_metadata_from_context_target() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
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

        assert!(scene.open_properties_overlay_from_context());
        let overlay = scene
            .properties_overlay
            .as_ref()
            .expect("properties overlay should open");
        assert_eq!(overlay.title, "Properties - Root");
        assert!(
            overlay
                .rows
                .iter()
                .any(|row| row.label == "Type" && row.value == "Place")
        );
        assert!(
            overlay
                .rows
                .iter()
                .any(|row| row.label == "Section" && row.value == "Devices")
        );
        assert!(
            overlay
                .rows
                .iter()
                .any(|row| row.label == "Root" && row.value == "Yes")
        );
        assert!(
            overlay
                .rows
                .iter()
                .any(|row| row.label == "Path" && row.value == "/")
        );
        assert_eq!(scene.properties_changes, 1);
    }

    #[test]
    fn create_dialog_opens_from_blank_context_and_accepts_text() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(420, 260);
        let root = test_dir("create-dialog");
        scene.context_target = Some(ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_0,
            path: root,
        });

        assert!(scene.open_create_dialog_from_context());
        let dialog = scene
            .create_dialog
            .as_ref()
            .expect("create dialog should open");
        assert_eq!(dialog.kind, CreateEntryKind::Folder);
        assert_eq!(dialog.name, "New Folder");
        assert_eq!(scene.create_changes, 1);

        assert!(scene.apply_create_command(CreateCommand::Insert("custom".to_string()), size));
        assert_eq!(scene.create_dialog.as_ref().unwrap().name, "custom");
        assert!(scene.apply_create_command(CreateCommand::SetKind(CreateEntryKind::File), size));
        let dialog = scene.create_dialog.as_ref().unwrap();
        assert_eq!(dialog.kind, CreateEntryKind::File);
        assert_eq!(dialog.name, "New File");
        assert_eq!(
            scene.create_dialog_click_at_screen_point(
                ViewPoint {
                    x: size.width as f32 + 1.0,
                    y: 1.0,
                },
                size,
            ),
            CreateDialogClick::Outside
        );
        let rect = create_dialog_rect(dialog, size);
        assert_eq!(
            scene.create_dialog_click_at_screen_point(
                ViewPoint {
                    x: create_dialog_commit_button_rect(rect).x + 2.0,
                    y: create_dialog_commit_button_rect(rect).y + 2.0,
                },
                size,
            ),
            CreateDialogClick::Commit
        );
    }

    #[test]
    fn create_entry_request_rejects_invalid_names_and_records_error() {
        let mut scene = test_scene(vec![], ShellViewMode::Icons);
        scene.context_target = Some(ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_0,
            path: PathBuf::from("/tmp"),
        });
        assert!(scene.open_create_dialog_from_context());
        scene.create_dialog.as_mut().unwrap().name = "../bad".to_string();

        let error = scene.create_entry_request().unwrap_err();
        assert!(error.contains('/'));
        assert!(scene.set_create_dialog_error(error));
        let dialog = scene.create_dialog.as_ref().unwrap();
        assert!(dialog.error.as_ref().unwrap().contains('/'));
        assert_eq!(scene.create_changes, 2);
    }

    #[test]
    fn create_and_rename_requests_preserve_explicit_administrator_flag() {
        let mut scene = test_scene(vec![test_entry("plain.txt", false)], ShellViewMode::Icons);
        scene.context_target = Some(ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_0,
            path: PathBuf::from("/tmp"),
        });
        assert!(scene.open_create_dialog_from_context_with_kind(CreateEntryKind::File, true));
        scene.create_dialog.as_mut().unwrap().name = "admin.txt".to_string();
        let create_request = scene.create_entry_request().unwrap();
        assert!(create_request.privileged);

        scene.context_target = Some(ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: PathBuf::from("/tmp/plain.txt"),
            is_dir: false,
            selection_count: 1,
        });
        assert!(scene.open_rename_dialog_from_context(true));
        scene.rename_dialog.as_mut().unwrap().name = "admin-renamed.txt".to_string();
        let rename_request = scene.rename_entry_request().unwrap();
        assert!(rename_request.privileged);
    }

    #[test]
    fn create_new_folder_creates_on_disk_reloads_and_selects_entry() {
        let root = test_dir("create-folder");
        fs::create_dir_all(&root).unwrap();
        let size = PhysicalSize::new(420, 260);
        let mut scene = ShellScene::load(root.clone(), ShellViewMode::Icons).unwrap();
        scene.context_target = Some(ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_0,
            path: root.clone(),
        });

        assert!(scene.open_create_dialog_from_context());
        scene.create_dialog.as_mut().unwrap().name = "made".to_string();
        scene.create_dialog.as_mut().unwrap().replace_on_insert = false;
        let request = scene.create_entry_request().unwrap();
        assert_eq!(request.kind, CreateEntryKind::Folder);
        create_entry_on_disk(&request).unwrap();
        assert!(root.join("made").is_dir());
        assert!(scene.close_create_dialog_after_success(&request));
        assert!(scene.reload_current_path(size).unwrap());
        assert!(scene.select_entry_by_name("made", size));

        let index = entry_index_by_name(&scene.panes[ShellPaneId::SLOT_0].entries, "made").unwrap();
        assert!(scene.panes[ShellPaneId::SLOT_0].entries[index].is_dir);
        assert!(scene.panes[ShellPaneId::SLOT_0].selection.contains(index));
        assert!(scene.create_dialog.is_none());
        assert_eq!(scene.directory_reloads, 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn create_new_file_uses_create_new_and_unique_default_name() {
        let root = test_dir("create-file");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("New File"), b"existing").unwrap();
        let mut dialog = ShellCreateDialog::new(
            ShellPaneId::SLOT_0,
            root.clone(),
            CreateEntryKind::File,
            false,
        );
        assert_eq!(dialog.name, "New File 2");
        dialog.name = "note.txt".to_string();
        let request = CreateEntryRequest {
            pane: ShellPaneId::SLOT_0,
            parent: root.clone(),
            path: root.join("note.txt"),
            kind: CreateEntryKind::File,
            name: "note.txt".to_string(),
            privileged: false,
        };

        create_entry_on_disk(&request).unwrap();
        assert!(root.join("note.txt").is_file());
        assert!(
            create_entry_on_disk(&request)
                .unwrap_err()
                .contains("create file")
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rename_dialog_opens_from_item_context_and_accepts_text() {
        let mut scene = test_scene(vec![test_entry("plain.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(420, 260);
        scene.context_target = Some(ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: PathBuf::from("/tmp/plain.txt"),
            is_dir: false,
            selection_count: 1,
        });

        assert!(scene.open_rename_dialog_from_context(false));
        let dialog = scene
            .rename_dialog
            .as_ref()
            .expect("rename dialog should open");
        assert_eq!(dialog.original_name, "plain.txt");
        assert_eq!(dialog.name, "plain.txt");
        assert!(!dialog.is_dir);
        assert_eq!(scene.rename_changes, 1);

        assert!(scene.apply_rename_command(RenameCommand::Insert("renamed.txt".to_string())));
        assert_eq!(scene.rename_dialog.as_ref().unwrap().name, "renamed.txt");
        let rect = rename_dialog_rect(scene.rename_dialog.as_ref().unwrap(), size);
        assert_eq!(
            scene.rename_dialog_click_at_screen_point(
                ViewPoint {
                    x: rename_dialog_commit_button_rect(rect).x + 2.0,
                    y: rename_dialog_commit_button_rect(rect).y + 2.0,
                },
                size,
            ),
            RenameDialogClick::Commit
        );
        assert_eq!(
            scene.rename_dialog_click_at_screen_point(
                ViewPoint {
                    x: size.width as f32 + 1.0,
                    y: 1.0,
                },
                size,
            ),
            RenameDialogClick::Outside
        );
    }

    #[test]
    fn rename_entry_request_rejects_unchanged_and_invalid_names() {
        let mut scene = test_scene(vec![test_entry("plain.txt", false)], ShellViewMode::Icons);
        scene.context_target = Some(ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: PathBuf::from("/tmp/plain.txt"),
            is_dir: false,
            selection_count: 1,
        });
        assert!(scene.open_rename_dialog_from_context(false));

        let unchanged = scene.rename_entry_request().unwrap_err();
        assert!(unchanged.contains("unchanged"));
        assert!(scene.set_rename_dialog_error(unchanged));
        assert!(scene.apply_rename_command(RenameCommand::Insert("../bad".to_string())));
        let invalid = scene.rename_entry_request().unwrap_err();
        assert!(invalid.contains('/'));
        assert!(scene.set_rename_dialog_error(invalid));
        assert!(scene.rename_dialog.as_ref().unwrap().error.is_some());
    }

    #[test]
    fn rename_file_creates_request_renames_on_disk_reloads_and_selects_entry() {
        let root = test_dir("rename-file");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("old.txt"), b"old").unwrap();
        let size = PhysicalSize::new(420, 260);
        let mut scene = ShellScene::load(root.clone(), ShellViewMode::Icons).unwrap();
        let old_index =
            entry_index_by_name(&scene.panes[ShellPaneId::SLOT_0].entries, "old.txt").unwrap();
        scene.context_target = Some(ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: old_index,
            path: root.join("old.txt"),
            is_dir: false,
            selection_count: 1,
        });

        assert!(scene.open_rename_dialog_from_context(false));
        scene.rename_dialog.as_mut().unwrap().name = "new.txt".to_string();
        scene.rename_dialog.as_mut().unwrap().replace_on_insert = false;
        let request = scene.rename_entry_request().unwrap();
        assert_eq!(request.original_name, "old.txt");
        assert_eq!(request.name, "new.txt");
        rename_entry_on_disk(&request).unwrap();
        assert!(!root.join("old.txt").exists());
        assert!(root.join("new.txt").is_file());
        assert!(scene.close_rename_dialog_after_success(&request));
        assert!(scene.reload_current_path(size).unwrap());
        assert!(scene.select_entry_by_name("new.txt", size));

        let new_index =
            entry_index_by_name(&scene.panes[ShellPaneId::SLOT_0].entries, "new.txt").unwrap();
        assert!(
            scene.panes[ShellPaneId::SLOT_0]
                .selection
                .contains(new_index)
        );
        assert!(scene.rename_dialog.is_none());
        assert_eq!(scene.directory_reloads, 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn split_pane_create_rename_clipboard_paste_and_trash_are_pane_local() {
        let root = test_dir("split-file-ops");
        let left = root.join("left");
        let right = root.join("right");
        let source_root = root.join("source");
        fs::create_dir_all(&left).unwrap();
        fs::create_dir_all(&right).unwrap();
        fs::create_dir_all(&source_root).unwrap();
        fs::write(left.join("left.txt"), b"left").unwrap();
        fs::write(right.join("existing.txt"), b"existing").unwrap();
        fs::write(right.join("trash-me.txt"), b"trash").unwrap();
        fs::write(source_root.join("copy.txt"), b"copy").unwrap();

        let size = PhysicalSize::new(900, 360);
        let mut scene = ShellScene::load(left.clone(), ShellViewMode::Icons).unwrap();
        assert!(scene.open_split_pane(right.clone(), size).unwrap());
        scene.active_pane = ShellPaneId::SLOT_1;

        scene.context_target = Some(ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_1,
            path: right.clone(),
        });
        assert!(scene.open_create_dialog_from_context_with_kind(CreateEntryKind::File, false));
        scene.create_dialog.as_mut().unwrap().name = "made.txt".to_string();
        scene.create_dialog.as_mut().unwrap().replace_on_insert = false;
        let create_request = scene.create_entry_request().unwrap();
        assert_eq!(create_request.pane, ShellPaneId::SLOT_1);
        create_entry_on_disk(&create_request).unwrap();
        assert!(scene.close_create_dialog_after_success(&create_request));
        assert!(scene.reload_pane_path(create_request.pane, size).unwrap());
        assert!(scene.select_entry_by_name_in_pane(
            create_request.pane,
            &create_request.name,
            size
        ));
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].path, left);
        assert!(
            entry_index_by_name(&scene.panes[ShellPaneId::SLOT_0].entries, "made.txt").is_none()
        );

        let made_index =
            entry_index_by_name(&scene.panes[ShellPaneId::SLOT_1].entries, "made.txt").unwrap();
        scene.context_target = Some(ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_1,
            index: made_index,
            path: right.join("made.txt"),
            is_dir: false,
            selection_count: 1,
        });
        assert!(scene.open_rename_dialog_from_context(false));
        scene.rename_dialog.as_mut().unwrap().name = "renamed.txt".to_string();
        scene.rename_dialog.as_mut().unwrap().replace_on_insert = false;
        let rename_request = scene.rename_entry_request().unwrap();
        assert_eq!(rename_request.pane, ShellPaneId::SLOT_1);
        rename_entry_on_disk(&rename_request).unwrap();
        assert!(scene.close_rename_dialog_after_success(&rename_request));
        assert!(scene.reload_pane_path(rename_request.pane, size).unwrap());
        assert!(scene.select_entry_by_name_in_pane(
            rename_request.pane,
            &rename_request.name,
            size
        ));
        assert!(right.join("renamed.txt").is_file());
        assert!(!right.join("made.txt").exists());
        assert!(
            entry_index_by_name(&scene.panes[ShellPaneId::SLOT_0].entries, "renamed.txt").is_none()
        );

        scene.rename_dialog = None;
        assert!(scene.open_rename_dialog_from_active_selection(false));
        let dialog = scene.rename_dialog.as_ref().unwrap();
        assert_eq!(dialog.pane, ShellPaneId::SLOT_1);
        assert_eq!(dialog.source, right.join("renamed.txt"));
        assert!(scene.close_rename_dialog());

        let clipboard_request = scene
            .active_file_clipboard_request(FileClipboardRole::Copy)
            .unwrap()
            .expect("active slot 1 pane selection should export paths");
        assert_eq!(clipboard_request.role, FileClipboardRole::Copy);
        assert_eq!(clipboard_request.paths, vec![right.join("renamed.txt")]);

        let paste_text =
            encode_file_clipboard_text(FileClipboardRole::Copy, &[source_root.join("copy.txt")]);
        let paste_result = scene
            .paste_clipboard_text_into_active_pane(&paste_text, size, false)
            .unwrap();
        assert_eq!(paste_result.mode, FileTransferMode::Copy);
        assert_eq!(paste_result.success_count, 1);
        assert_eq!(scene.paste_changes, 1);
        assert!(right.join("copy.txt").is_file());
        assert!(
            entry_index_by_name(&scene.panes[ShellPaneId::SLOT_1].entries, "copy.txt").is_some()
        );
        assert!(
            entry_index_by_name(&scene.panes[ShellPaneId::SLOT_0].entries, "copy.txt").is_none()
        );

        let trash_index =
            entry_index_by_name(&scene.panes[ShellPaneId::SLOT_1].entries, "trash-me.txt").unwrap();
        scene.context_target = Some(ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_1,
            index: trash_index,
            path: right.join("trash-me.txt"),
            is_dir: false,
            selection_count: 1,
        });
        let trash_result = scene.move_context_target_to_trash(size, false).unwrap();
        assert_eq!(trash_result.success_count, 1);
        assert_eq!(trash_result.failure_count, 0);
        assert!(!right.join("trash-me.txt").exists());
        assert!(
            entry_index_by_name(&scene.panes[ShellPaneId::SLOT_1].entries, "trash-me.txt")
                .is_none()
        );
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].path, left);

        file_ops::undo_trash(&trash_result.trash_pairs).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn split_panes_showing_same_dir_refresh_after_create_and_rename() {
        let root = test_dir("split-same-dir-file-ops");
        fs::create_dir_all(&root).unwrap();

        let size = PhysicalSize::new(900, 360);
        let mut scene = ShellScene::load(root.clone(), ShellViewMode::Icons).unwrap();
        assert!(scene.open_split_pane(root.clone(), size).unwrap());
        scene.active_pane = ShellPaneId::SLOT_1;

        scene.context_target = Some(ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_1,
            path: root.clone(),
        });
        assert!(scene.open_create_dialog_from_context_with_kind(CreateEntryKind::File, false));
        scene.create_dialog.as_mut().unwrap().name = "made.txt".to_string();
        scene.create_dialog.as_mut().unwrap().replace_on_insert = false;
        let create_request = scene.create_entry_request().unwrap();
        create_entry_on_disk(&create_request).unwrap();
        assert!(scene.close_create_dialog_after_success(&create_request));
        assert!(
            scene
                .reload_panes_showing_path(&create_request.parent, size)
                .unwrap()
        );
        assert!(scene.select_entry_by_name_in_pane(
            create_request.pane,
            &create_request.name,
            size
        ));

        assert!(
            entry_index_by_name(&scene.panes[ShellPaneId::SLOT_0].entries, "made.txt").is_some()
        );
        let made_index =
            entry_index_by_name(&scene.panes[ShellPaneId::SLOT_1].entries, "made.txt").unwrap();
        assert!(
            scene.panes[ShellPaneId::SLOT_1]
                .selection
                .contains(made_index)
        );
        assert_eq!(scene.directory_reloads, 2);

        scene.context_target = Some(ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_1,
            index: made_index,
            path: root.join("made.txt"),
            is_dir: false,
            selection_count: 1,
        });
        assert!(scene.open_rename_dialog_from_context(false));
        scene.rename_dialog.as_mut().unwrap().name = "renamed.txt".to_string();
        scene.rename_dialog.as_mut().unwrap().replace_on_insert = false;
        let rename_request = scene.rename_entry_request().unwrap();
        rename_entry_on_disk(&rename_request).unwrap();
        assert!(scene.close_rename_dialog_after_success(&rename_request));
        let rename_parent = rename_request.target.parent().unwrap().to_path_buf();
        assert!(
            scene
                .reload_panes_showing_path(&rename_parent, size)
                .unwrap()
        );
        assert!(scene.select_entry_by_name_in_pane(
            rename_request.pane,
            &rename_request.name,
            size
        ));

        assert!(
            entry_index_by_name(&scene.panes[ShellPaneId::SLOT_0].entries, "made.txt").is_none()
        );
        assert!(
            entry_index_by_name(&scene.panes[ShellPaneId::SLOT_0].entries, "renamed.txt").is_some()
        );
        assert!(
            entry_index_by_name(&scene.panes[ShellPaneId::SLOT_1].entries, "made.txt").is_none()
        );
        let renamed_index =
            entry_index_by_name(&scene.panes[ShellPaneId::SLOT_1].entries, "renamed.txt").unwrap();
        assert!(
            scene.panes[ShellPaneId::SLOT_1]
                .selection
                .contains(renamed_index)
        );
        assert_eq!(scene.directory_reloads, 4);

        fs::remove_dir_all(root).unwrap();
    }
