
    #[test]
    fn trash_context_target_uses_multi_selection_and_rejects_remote_paths() {
        let mut scene = test_scene(
            vec![
                test_entry("one.txt", false),
                test_entry("two.txt", false),
                test_entry("remote", false),
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

        assert_eq!(
            scene.context_target_trash_paths().unwrap(),
            vec![PathBuf::from("/tmp/one.txt"), PathBuf::from("/tmp/two.txt")]
        );

        scene.context_target = Some(ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 2,
            path: PathBuf::from("sftp://example.test/home/remote"),
            is_dir: false,
            selection_count: 1,
        });
        assert!(
            scene
                .move_context_target_to_trash(PhysicalSize::new(420, 260), false)
                .unwrap_err()
                .contains("remote trash")
        );
        assert_eq!(scene.trash_changes, 0);
    }

    #[test]
    fn trash_view_operation_requests_validate_context_targets() {
        let mut scene = test_scene(vec![test_entry("plain.txt", false)], ShellViewMode::Icons);
        let trash_path = file_ops::trash_files_dir().join("plain.txt");
        scene.context_target = Some(ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: trash_path.clone(),
            is_dir: false,
            selection_count: 1,
        });
        let (operation, paths) = scene
            .context_target_trash_view_operation(ShellContextMenuAction::RestoreFromTrash)
            .unwrap();
        assert!(matches!(operation, TrashViewOperation::Restore { .. }));
        assert_eq!(paths, vec![trash_path.clone()]);

        let (operation, paths) = scene
            .context_target_trash_view_operation(ShellContextMenuAction::DeletePermanently)
            .unwrap();
        assert_eq!(operation, TrashViewOperation::DeletePermanently);
        assert_eq!(paths, vec![trash_path]);

        scene.context_target = Some(ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_0,
            path: file_ops::trash_files_dir(),
        });
        let (operation, paths) = scene
            .context_target_trash_view_operation(ShellContextMenuAction::EmptyTrash)
            .unwrap();
        assert_eq!(operation, TrashViewOperation::Empty);
        assert!(paths.is_empty());

        scene.context_target = Some(ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: PathBuf::from("/tmp/plain.txt"),
            is_dir: false,
            selection_count: 1,
        });
        assert!(
            scene
                .context_target_trash_view_operation(ShellContextMenuAction::RestoreFromTrash)
                .unwrap_err()
                .contains("inside Trash")
        );
    }

    #[test]
    fn restore_trash_view_action_restores_test_file_and_reloads() {
        let root = test_dir("restore-trash-view");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("restore-me.txt");
        fs::write(&source, b"restore").unwrap();
        let summary = file_ops::trash_paths(std::slice::from_ref(&source));
        assert_eq!(summary.successes.len(), 1);
        let trash_path = summary.successes[0].trash_path.clone();
        let size = PhysicalSize::new(420, 260);
        let mut scene =
            ShellScene::load(file_ops::trash_files_dir(), ShellViewMode::Icons).unwrap();
        scene.context_target = Some(ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: trash_path.clone(),
            is_dir: false,
            selection_count: 1,
        });
        scene.context_menu = Some(ShellContextMenu::new(
            scene.context_target.clone().unwrap(),
            ViewPoint { x: 8.0, y: 8.0 },
        ));
        scene.panes[ShellPaneId::SLOT_0]
            .selection
            .select_indexes(&[0]);

        let result = scene
            .perform_trash_view_context_action(ShellContextMenuAction::RestoreFromTrash, size)
            .unwrap();

        assert_eq!(result.success_count, 1);
        assert_eq!(result.failure_count, 0);
        assert!(source.is_file());
        assert!(!trash_path.exists());
        assert!(scene.context_target.is_none());
        assert!(scene.context_menu.is_none());
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].selection.len(), 0);
        assert_eq!(scene.trash_changes, 1);
        assert_eq!(scene.directory_reloads, 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn trash_restore_conflict_dialog_replaces_existing_destination() {
        let root = test_dir("trash-restore-conflict");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("conflict.txt");
        fs::write(&source, b"old").unwrap();
        let summary = file_ops::trash_paths(std::slice::from_ref(&source));
        assert_eq!(summary.successes.len(), 1);
        let trash_path = summary.successes[0].trash_path.clone();
        fs::write(&source, b"new").unwrap();

        let size = PhysicalSize::new(520, 300);
        let mut scene =
            ShellScene::load(file_ops::trash_files_dir(), ShellViewMode::Icons).unwrap();
        scene.context_target = Some(ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: trash_path.clone(),
            is_dir: false,
            selection_count: 1,
        });

        let result = scene
            .perform_trash_view_context_action(ShellContextMenuAction::RestoreFromTrash, size)
            .unwrap();

        assert_eq!(result.success_count, 0);
        assert_eq!(result.restore_conflicts.len(), 1);
        assert_eq!(fs::read(&source).unwrap(), b"new");
        assert!(trash_path.exists());
        assert!(scene.trash_conflict_dialog.is_some());
        assert_eq!(scene.trash_changes, 1);
        assert_eq!(scene.directory_reloads, 0);

        let rect = trash_conflict_dialog_rect(scene.trash_conflict_dialog.as_ref().unwrap(), size);
        assert_eq!(
            scene.trash_conflict_dialog_click_at_screen_point(
                ViewPoint {
                    x: trash_conflict_dialog_replace_button_rect(rect).x + 2.0,
                    y: trash_conflict_dialog_replace_button_rect(rect).y + 2.0,
                },
                size,
            ),
            TrashConflictDialogClick::Replace
        );
        assert_eq!(
            scene.trash_conflict_dialog_click_at_screen_point(ViewPoint { x: 1.0, y: 1.0 }, size),
            TrashConflictDialogClick::Outside
        );

        let replace = scene.replace_trash_restore_conflicts(size).unwrap();

        assert_eq!(replace.success_count, 1);
        assert_eq!(replace.failure_count, 0);
        assert_eq!(fs::read(&source).unwrap(), b"old");
        assert!(!trash_path.exists());
        assert!(scene.trash_conflict_dialog.is_none());
        assert_eq!(scene.trash_changes, 2);
        assert_eq!(scene.directory_reloads, 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn delete_permanently_trash_view_action_deletes_test_file_and_reloads() {
        let root = test_dir("delete-trash-view");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("delete-me.txt");
        fs::write(&source, b"delete").unwrap();
        let summary = file_ops::trash_paths(std::slice::from_ref(&source));
        assert_eq!(summary.successes.len(), 1);
        let trash_path = summary.successes[0].trash_path.clone();
        let size = PhysicalSize::new(420, 260);
        let mut scene =
            ShellScene::load(file_ops::trash_files_dir(), ShellViewMode::Icons).unwrap();
        scene.context_target = Some(ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: trash_path.clone(),
            is_dir: false,
            selection_count: 1,
        });

        let result = scene
            .perform_trash_view_context_action(ShellContextMenuAction::DeletePermanently, size)
            .unwrap();

        assert_eq!(result.success_count, 1);
        assert_eq!(result.failure_count, 0);
        assert!(!source.exists());
        assert!(!trash_path.exists());
        assert_eq!(scene.trash_changes, 1);
        assert_eq!(scene.directory_reloads, 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn async_empty_trash_completion_replaces_running_status_and_reloads() {
        let root = test_dir("async-empty-trash");
        fs::create_dir_all(&root).unwrap();
        let size = PhysicalSize::new(420, 260);
        let mut scene = ShellScene::load(root.clone(), ShellViewMode::Icons).unwrap();
        scene.context_target = Some(ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_0,
            path: file_ops::trash_files_dir(),
        });
        scene.context_menu = Some(ShellContextMenu::new(
            scene.context_target.clone().unwrap(),
            ViewPoint { x: 8.0, y: 8.0 },
        ));

        scene.record_async_trash_view_started(77, TrashViewOperation::Empty, 0);

        assert!(scene.context_target.is_none());
        assert!(scene.context_menu.is_none());
        assert_eq!(scene.task_statuses[0].kind, ShellTaskStatusKind::Running);
        assert_eq!(scene.task_statuses[0].label, "Emptying Trash");
        assert!(!scene.task_statuses[0].cancellable);

        let completion = ShellAsyncTrashViewCompletion {
            task_id: 77,
            action: ShellContextMenuAction::EmptyTrash,
            pane_to_reload: ShellPaneId::SLOT_0,
            result: TrashViewOperationResult {
                pane_id: SHELL_PANE_ID,
                operation: TrashViewOperation::Empty,
                success_count: 1,
                failure_count: 0,
                affected_dirs: vec![root.clone()],
                restore_conflicts: Vec::new(),
            },
        };

        scene
            .apply_async_trash_view_completion(&completion, size)
            .unwrap();

        assert_eq!(scene.task_statuses[0].task_id, Some(77));
        assert_eq!(scene.task_statuses[0].kind, ShellTaskStatusKind::Completed);
        assert_eq!(scene.task_statuses[0].label, "Empty Trash");
        assert_eq!(scene.task_statuses[0].detail, "1 item");
        assert_eq!(scene.trash_changes, 1);
        assert_eq!(scene.directory_reloads, 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn move_to_trash_moves_context_target_reloads_and_clears_selection() {
        let root = test_dir("trash-file");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("remove.txt"), b"remove").unwrap();
        fs::write(root.join("keep.txt"), b"keep").unwrap();
        let size = PhysicalSize::new(420, 260);
        let mut scene = ShellScene::load(root.clone(), ShellViewMode::Icons).unwrap();
        let remove_index =
            entry_index_by_name(&scene.panes[ShellPaneId::SLOT_0].entries, "remove.txt").unwrap();
        scene.panes[ShellPaneId::SLOT_0]
            .selection
            .select_indexes(&[remove_index]);
        scene.context_target = Some(ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: remove_index,
            path: root.join("remove.txt"),
            is_dir: false,
            selection_count: 1,
        });

        let result = scene.move_context_target_to_trash(size, false).unwrap();
        assert_eq!(result.success_count, 1);
        assert_eq!(result.failure_count, 0);
        assert_eq!(scene.trash_changes, 1);
        assert_eq!(scene.directory_reloads, 1);
        assert!(!root.join("remove.txt").exists());
        assert!(root.join("keep.txt").exists());
        assert!(
            entry_index_by_name(&scene.panes[ShellPaneId::SLOT_0].entries, "remove.txt").is_none()
        );
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].selection.len(), 0);
        assert!(scene.context_target.is_none());

        file_ops::undo_trash(&result.trash_pairs).unwrap();
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn active_delete_reloads_active_split_trash_view() {
        file_ops::ensure_trash_dirs().unwrap();
        let root = test_dir("split-active-delete");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("left.txt"), b"left").unwrap();
        let trash_dir = file_ops::trash_files_dir();
        let unique = format!(
            "tensor-files-active-delete-{}-{}.tmp",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let trash_file = trash_dir.join(unique);
        fs::write(&trash_file, b"delete").unwrap();

        let size = PhysicalSize::new(900, 360);
        let mut scene = ShellScene::load(root.clone(), ShellViewMode::Icons).unwrap();
        assert!(scene.open_split_pane(trash_dir.clone(), size).unwrap());
        scene.active_pane = ShellPaneId::SLOT_1;
        let trash_index = entry_index_by_name(
            &scene.panes[ShellPaneId::SLOT_1].entries,
            trash_file.file_name().unwrap().to_string_lossy().as_ref(),
        )
        .unwrap();
        scene.panes[ShellPaneId::SLOT_1]
            .selection
            .select_indexes(&[trash_index]);

        assert!(scene.delete_active_selection(size).unwrap());

        assert!(!trash_file.exists());
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].path, root);
        assert_eq!(scene.panes[ShellPaneId::SLOT_1].path, trash_dir);
        assert!(
            entry_index_by_name(
                &scene.panes[ShellPaneId::SLOT_1].entries,
                trash_file.file_name().unwrap().to_string_lossy().as_ref()
            )
            .is_none()
        );
        assert_eq!(scene.panes[ShellPaneId::SLOT_1].selection.len(), 0);
        assert_eq!(scene.directory_reloads, 1);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn selected_directory_path_uses_focus_and_target_path() {
        let target = PathBuf::from("/run/user/1000/gvfs/sftp:host=example");
        let mut scene = test_scene(
            vec![
                test_entry_with_target("remote", true, target.clone()),
                test_entry("plain.txt", false),
            ],
            ShellViewMode::Icons,
        );

        assert_eq!(scene.selected_directory_path(), None);
        assert!(
            scene.panes[ShellPaneId::SLOT_0]
                .selection
                .apply_navigation(0, false)
        );
        assert_eq!(
            scene.selected_directory_path(),
            Some((ShellPaneId::SLOT_0, target))
        );
        assert!(
            scene.panes[ShellPaneId::SLOT_0]
                .selection
                .apply_navigation(1, false)
        );
        assert_eq!(scene.selected_directory_path(), None);
    }

    #[test]
    fn double_click_directory_activation_uses_retained_hit_test() {
        let mut scene = test_scene(vec![test_entry("folder", true)], ShellViewMode::Icons);
        let size = PhysicalSize::new(360, 240);
        let item = scene.layout(size).item(0).expect("test item should layout");
        let point = ViewPoint {
            x: scene.content_origin_x(size) + item.visual_rect.x + 4.0,
            y: item.visual_rect.y + scene.content_origin_y() + 4.0,
        };
        let now = Instant::now();

        assert_eq!(scene.item_activation_for_press(point, size, now), None);
        assert_eq!(
            scene.item_activation_for_press(point, size, now + Duration::from_millis(120)),
            Some(ShellItemActivation::Directory {
                pane: ShellPaneId::SLOT_0,
                path: PathBuf::from("/tmp/folder")
            })
        );
    }

    #[test]
    fn double_click_file_activation_uses_default_app_like_file_manager() {
        let mut scene = test_scene(vec![test_entry("plain.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(360, 240);
        let item = scene.layout(size).item(0).expect("test item should layout");
        let point = ViewPoint {
            x: scene.content_origin_x(size) + item.visual_rect.x + 4.0,
            y: item.visual_rect.y + scene.content_origin_y() + 4.0,
        };
        let now = Instant::now();

        assert_eq!(scene.item_activation_for_press(point, size, now), None);
        assert_eq!(
            scene.item_activation_for_press(point, size, now + Duration::from_millis(120)),
            Some(ShellItemActivation::File(OpenFileRequest {
                path: PathBuf::from("/tmp/plain.txt"),
                uri: "file:///tmp/plain.txt".to_string(),
                mime_type: Some("text/plain".to_string()),
            }))
        );
    }

    #[test]
    fn load_path_replaces_entries_and_resets_transient_state() {
        let unique = format!(
            "tensor-files-load-path-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = env::temp_dir().join(unique);
        let child = root.join("child");
        fs::create_dir_all(&child).unwrap();
        fs::write(child.join("nested.txt"), b"nested").unwrap();

        let size = PhysicalSize::new(360, 240);
        let mut scene = ShellScene::load(root.clone(), ShellViewMode::Compact).unwrap();
        scene.panes[ShellPaneId::SLOT_0].scroll_x = 128.0;
        scene.panes[ShellPaneId::SLOT_0].scroll_y = 64.0;
        scene.pointer = Some(ViewPoint {
            x: 12.0,
            y: TOP_BAR_HEIGHT + 12.0,
        });
        assert!(
            scene.panes[ShellPaneId::SLOT_0]
                .selection
                .apply_navigation(0, false)
        );
        scene.rubber_band = Some(RubberBand::new(
            ViewPoint { x: 0.0, y: 0.0 },
            RubberBandMode::Replace,
            ShellSelection::default(),
        ));

        scene.load_path(child.clone(), size).unwrap();

        assert_eq!(scene.panes[ShellPaneId::SLOT_0].path, child);
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].entries.len(), 1);
        assert_eq!(
            scene.panes[ShellPaneId::SLOT_0].entries[0].name.as_ref(),
            "nested.txt"
        );
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].scroll_x, 0.0);
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].scroll_y, 0.0);
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].selection.len(), 0);
        assert!(scene.rubber_band.is_none());
        assert_eq!(scene.path_changes, 1);
        assert!(!scene.animation_active());

        fs::remove_dir_all(root).unwrap();
    }
