
    #[test]
    fn text_input_batch_replaces_location_selection_and_keeps_preedit_separate() {
        let mut scene = TestShellSceneBuilder::new().build();
        let size = PhysicalSize::new(640, 420);
        assert!(scene.apply_location_command(LocationCommand::Activate, size));

        let outcome = scene.apply_location_text_input(
            ShellTextInputBatch {
                commit: Some("目录".to_string()),
                preedit: ShellTextPreedit::new("中".to_string(), Some(3..3)),
                ..ShellTextInputBatch::default()
            },
            size,
        );

        let location = scene.location_draft.as_ref().unwrap();
        assert!(outcome.content_changed);
        assert_eq!(location.draft.value, "目录");
        assert_eq!(location.draft.cursor, "目录".len());
        assert_eq!(scene.location_label_for_pane(location.pane), "目录中");
    }

    #[test]
    fn text_input_batch_updates_create_name_and_clears_preedit_on_leave() {
        let mut scene = TestShellSceneBuilder::new().build();
        scene.create_dialog = Some(ShellCreateDialog::new(
            ShellPaneId::SLOT_0,
            PathBuf::from("/tmp"),
            CreateEntryKind::Folder,
            false,
        ));

        let outcome = scene.apply_create_text_input(ShellTextInputBatch {
            commit: Some("项目".to_string()),
            preedit: ShellTextPreedit::new("一".to_string(), None),
            ..ShellTextInputBatch::default()
        });
        assert!(outcome.content_changed);
        assert_eq!(scene.create_dialog.as_ref().unwrap().name, "项目");
        assert!(scene.create_dialog.as_ref().unwrap().preedit.is_some());

        let cleared = scene.apply_create_text_input(ShellTextInputBatch::default());
        assert!(cleared.visual_changed);
        assert!(scene.create_dialog.as_ref().unwrap().preedit.is_none());
    }

    #[test]
    fn text_input_batch_recomputes_open_with_filter_from_committed_query() {
        let mut scene = TestShellSceneBuilder::new().build();
        scene.open_with_chooser = Some(ShellOpenWithChooser::new(
            PathBuf::from("/tmp/note.txt"),
            None,
            Vec::new(),
            Vec::new(),
        ));

        let outcome = scene.apply_open_with_text_input(ShellTextInputBatch {
            commit: Some("编辑器".to_string()),
            ..ShellTextInputBatch::default()
        });

        let chooser = scene.open_with_chooser.as_ref().unwrap();
        assert!(outcome.content_changed);
        assert_eq!(chooser.query, "编辑器");
        assert_eq!(chooser.query_cursor, "编辑器".len());
        assert_eq!(chooser.scroll_row, 0);
    }
