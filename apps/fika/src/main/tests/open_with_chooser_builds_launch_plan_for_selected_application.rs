
    #[test]
    fn open_with_chooser_builds_launch_plan_for_selected_application() {
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
                is_default: true,
            }],
            Vec::new(),
        ));
        assert!(scene.select_open_with_filtered_row(0));
        assert!(scene.select_open_with_filtered_row(1));
        let cache = MimeApplicationCache::from_applications_and_mimeapps(
            vec![test_desktop_application(
                "writer.desktop",
                "Writer",
                "writer --line %f",
                &["text/plain"],
            )],
            &[],
        );

        let request = scene.open_with_launch_request(&cache).unwrap();

        assert_eq!(request.path, PathBuf::from("/tmp/note.txt"));
        assert_eq!(request.app_name, "Writer");
        assert_eq!(request.plan.commands[0].program, "writer");
        assert_eq!(
            request.plan.commands[0].args,
            vec!["--line", "/tmp/note.txt"]
        );
        assert_eq!(request.default_update, None);
    }

    #[test]
    fn open_with_chooser_launch_request_can_set_selected_application_as_default() {
        let mut scene = test_scene(vec![test_entry("note.txt", false)], ShellViewMode::Icons);
        scene.open_with_chooser = Some(ShellOpenWithChooser::new(
            PathBuf::from("/tmp/note.txt"),
            Some(Arc::from("text/plain")),
            vec![MimeApplication {
                id: "viewer.desktop".to_string(),
                desktop_file: PathBuf::from("/apps/viewer.desktop"),
                name: "Viewer".to_string(),
                exec: "viewer %f".to_string(),
                icon: None,
                is_default: false,
            }],
            Vec::new(),
        ));
        assert!(scene.select_open_with_filtered_row(0));
        assert!(scene.select_open_with_filtered_row(1));
        assert!(scene.toggle_open_with_set_default());
        let cache = MimeApplicationCache::from_applications_and_mimeapps(
            vec![test_desktop_application(
                "viewer.desktop",
                "Viewer",
                "viewer %f",
                &["text/plain"],
            )],
            &[],
        );

        let request = scene.open_with_launch_request(&cache).unwrap();

        assert_eq!(
            request.default_update,
            Some(OpenWithDefaultUpdate {
                mime_type: "text/plain".to_string(),
                desktop_id: "viewer.desktop".to_string(),
            })
        );
    }

    #[test]
    fn ark_builtin_context_action_compresses_local_multi_selection() {
        let mut scene = test_scene(
            vec![test_entry("one.txt", false), test_entry("two.txt", false)],
            ShellViewMode::Icons,
        );
        assert!(
            scene.panes[ShellPaneId::SLOT_0]
                .selection
                .select_indexes(&[0, 1])
        );
        let target = ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: PathBuf::from("/tmp/one.txt"),
            is_dir: false,
            selection_count: 2,
        };

        let (_, actions) = scene.context_menu_dynamic_data(&target, &MimeApplicationCache::empty());

        assert!(actions.iter().any(|action| {
            action.id == BUILTIN_ARK_COMPRESS_TAR_GZ_ACTION_ID
                && action.label == "Compress to \"Archive.tar.gz\""
                && action.submenu.as_deref() == Some(BUILTIN_ARK_COMPRESS_SUBMENU)
        }));
        assert!(actions.iter().any(|action| {
            action.id == BUILTIN_ARK_COMPRESS_ZIP_ACTION_ID
                && action.label == "Compress to \"Archive.zip\""
                && action.submenu.as_deref() == Some(BUILTIN_ARK_COMPRESS_SUBMENU)
        }));
        assert!(actions.iter().any(|action| {
            action.id == BUILTIN_ARK_COMPRESS_ACTION_ID
                && action.label == "Compress to…"
                && action.submenu.as_deref() == Some(BUILTIN_ARK_COMPRESS_SUBMENU)
        }));
        assert!(
            actions
                .iter()
                .all(|action| action.id != BUILTIN_ARK_EXTRACT_HERE_ACTION_ID)
        );

        let menu = ShellContextMenu::with_dynamic(
            target.clone(),
            ViewPoint { x: 20.0, y: 20.0 },
            Vec::new(),
            actions,
        );
        let root = context_menu_items(&menu);
        let compress_submenu = root
            .iter()
            .find_map(|item| {
                (item.label == BUILTIN_ARK_COMPRESS_SUBMENU).then_some(item.submenu)?
            })
            .expect("Ark compress actions should be a root submenu");
        let compress_items = context_submenu_actions(compress_submenu, &menu);
        assert_eq!(
            compress_items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Compress to \"Archive.tar.gz\"",
                "Compress to \"Archive.zip\"",
                "Compress to…"
            ]
        );

        scene.context_target = Some(target);
        let request = scene
            .service_menu_launch_request(
                &MimeApplicationCache::empty(),
                BUILTIN_ARK_COMPRESS_ZIP_ACTION_ID,
            )
            .unwrap();

        assert_eq!(
            request.paths,
            vec![PathBuf::from("/tmp/one.txt"), PathBuf::from("/tmp/two.txt")]
        );
        assert_eq!(request.app_name, "Ark: Compress to ZIP");
        assert_eq!(request.plan.commands[0].program, "ark");
        assert_eq!(
            request.plan.commands[0].args,
            vec![
                "--add",
                "--changetofirstpath",
                "--autofilename",
                "zip",
                "/tmp/one.txt",
                "/tmp/two.txt"
            ]
        );

        let dialog = scene
            .service_menu_launch_request(
                &MimeApplicationCache::empty(),
                BUILTIN_ARK_COMPRESS_ACTION_ID,
            )
            .unwrap();
        assert_eq!(dialog.app_name, "Ark: Compress To");
        assert_eq!(
            dialog.plan.commands[0].args,
            vec![
                "--add",
                "--changetofirstpath",
                "--dialog",
                "/tmp/one.txt",
                "/tmp/two.txt"
            ]
        );
    }

    #[test]
    fn ark_builtin_context_action_extracts_single_archive_by_mime() {
        let mut scene = test_scene(
            vec![test_entry_with_mime(
                "payload.bin",
                false,
                "application/zip",
            )],
            ShellViewMode::Icons,
        );
        let target = ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: PathBuf::from("/tmp/payload.bin"),
            is_dir: false,
            selection_count: 1,
        };

        let (_, actions) = scene.context_menu_dynamic_data(&target, &MimeApplicationCache::empty());

        assert!(
            actions
                .iter()
                .all(|action| action.submenu.as_deref() != Some(BUILTIN_ARK_COMPRESS_SUBMENU))
        );
        assert!(
            actions
                .iter()
                .any(|action| action.id == BUILTIN_ARK_EXTRACT_HERE_ACTION_ID
                    && action.label == "Extract here"
                    && action.submenu.as_deref() == Some(BUILTIN_ARK_EXTRACT_SUBMENU))
        );
        assert!(actions.iter().any(
            |action| action.id == BUILTIN_ARK_EXTRACT_AND_TRASH_ACTION_ID
                && action.label == "Extract and trash archive"
                && action.submenu.as_deref() == Some(BUILTIN_ARK_EXTRACT_SUBMENU)
        ));
        assert!(
            actions
                .iter()
                .any(|action| action.id == BUILTIN_ARK_EXTRACT_TO_ACTION_ID
                    && action.label == "Extract to…"
                    && action.submenu.as_deref() == Some(BUILTIN_ARK_EXTRACT_SUBMENU))
        );

        let menu = ShellContextMenu::with_dynamic(
            target.clone(),
            ViewPoint { x: 20.0, y: 20.0 },
            Vec::new(),
            actions,
        );
        let root = context_menu_items(&menu);
        let extract_submenu = root
            .iter()
            .find_map(|item| (item.label == BUILTIN_ARK_EXTRACT_SUBMENU).then_some(item.submenu)?)
            .expect("Ark extract actions should be a root submenu");
        let extract_items = context_submenu_actions(extract_submenu, &menu);
        assert_eq!(
            extract_items
                .iter()
                .map(|item| item.label.as_str())
                .collect::<Vec<_>>(),
            vec!["Extract here", "Extract and trash archive", "Extract to…"]
        );

        scene.context_target = Some(target);
        let request = scene
            .service_menu_launch_request(
                &MimeApplicationCache::empty(),
                BUILTIN_ARK_EXTRACT_HERE_ACTION_ID,
            )
            .unwrap();

        assert_eq!(request.paths, vec![PathBuf::from("/tmp/payload.bin")]);
        assert_eq!(request.app_name, "Ark: Extract Here");
        assert_eq!(
            request.plan.commands[0].args,
            vec![
                "--batch",
                "--autosubfolder",
                "--destination",
                "/tmp",
                "/tmp/payload.bin"
            ]
        );
    }

    #[test]
    fn ark_builtin_context_action_extracts_selected_archives_only() {
        let mut scene = test_scene(
            vec![
                test_entry_with_mime("one.zip", false, "application/zip"),
                test_entry("notes.txt", false),
                test_entry_with_mime("two.tar.gz", false, "application/x-compressed-tar"),
            ],
            ShellViewMode::Icons,
        );
        assert!(
            scene.panes[ShellPaneId::SLOT_0]
                .selection
                .select_indexes(&[0, 1, 2])
        );
        let target = ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: PathBuf::from("/tmp/one.zip"),
            is_dir: false,
            selection_count: 3,
        };

        let (_, actions) = scene.context_menu_dynamic_data(&target, &MimeApplicationCache::empty());
        assert!(
            actions
                .iter()
                .any(|action| action.id == BUILTIN_ARK_EXTRACT_HERE_ACTION_ID)
        );

        scene.context_target = Some(target);
        let request = scene
            .service_menu_launch_request(
                &MimeApplicationCache::empty(),
                BUILTIN_ARK_EXTRACT_HERE_ACTION_ID,
            )
            .unwrap();

        assert_eq!(
            request.paths,
            vec![
                PathBuf::from("/tmp/one.zip"),
                PathBuf::from("/tmp/two.tar.gz")
            ]
        );
        assert_eq!(
            request.plan.commands[0].args,
            vec![
                "--batch",
                "--autosubfolder",
                "--destination",
                "/tmp",
                "/tmp/one.zip",
                "/tmp/two.tar.gz"
            ]
        );
    }

    #[test]
    fn ark_builtin_context_actions_skip_remote_targets() {
        let scene = test_scene(
            vec![test_entry_with_mime(
                "archive.zip",
                false,
                "application/zip",
            )],
            ShellViewMode::Icons,
        );
        let target = ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: PathBuf::from("smb://server/share/archive.zip"),
            is_dir: false,
            selection_count: 1,
        };

        let (_, actions) = scene.context_menu_dynamic_data(&target, &MimeApplicationCache::empty());

        assert!(
            actions
                .iter()
                .all(|action| !action.id.starts_with("fika.builtin.ark."))
        );
    }

    #[test]
    fn trash_context_menu_uses_restore_delete_and_empty_actions() {
        let trash_item = ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: file_ops::trash_files_dir().join("trashed.txt"),
            is_dir: false,
            selection_count: 1,
        };
        assert_eq!(
            context_menu_actions(&trash_item),
            &[
                ShellContextMenuAction::RestoreFromTrash,
                ShellContextMenuAction::Copy,
                ShellContextMenuAction::DeletePermanently,
                ShellContextMenuAction::Properties,
            ]
        );

        let trash_blank = ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_0,
            path: file_ops::trash_files_dir(),
        };
        assert_eq!(
            context_menu_actions(&trash_blank),
            &[
                ShellContextMenuAction::EmptyTrash,
                ShellContextMenuAction::SelectAll,
                ShellContextMenuAction::Refresh,
                ShellContextMenuAction::Properties,
            ]
        );

        let trash_place = ShellContextTarget::Place {
            index: 0,
            label: "Trash".to_string(),
            path: file_ops::trash_files_dir(),
            group: "",
            device: None,
            network: false,
            trash: true,
            root: false,
            editable: false,
        };
        assert_eq!(
            context_menu_actions(&trash_place),
            &[
                ShellContextMenuAction::OpenInNewPane,
                ShellContextMenuAction::EmptyTrash,
                ShellContextMenuAction::CopyLocation,
                ShellContextMenuAction::Properties,
            ]
        );

        let normal_blank = ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_0,
            path: PathBuf::from("/tmp"),
        };
        assert!(!context_menu_actions(&normal_blank).contains(&ShellContextMenuAction::EmptyTrash));
    }

    #[test]
    fn build_shell_places_applies_persistent_left_order() {
        let root = test_dir("build-shell-places-order");
        let places_path = root.join("places.xbel");
        let alpha = root.join("alpha");
        let beta = root.join("beta");
        fs::create_dir_all(&alpha).unwrap();
        fs::create_dir_all(&beta).unwrap();
        save_user_places(
            &places_path,
            &[
                UserPlace::new("Alpha".to_string(), alpha.clone()),
                UserPlace::new("Beta".to_string(), beta.clone()),
            ],
        )
        .unwrap();
        save_place_order(
            &place_order_path_for_user_places_path(&places_path),
            &[beta.clone(), alpha.clone()],
        )
        .unwrap();

        let places = build_shell_places_from(&places_path);
        let beta_index = places
            .iter()
            .position(|place| place.path == beta)
            .expect("beta place should be loaded");
        let alpha_index = places
            .iter()
            .position(|place| place.path == alpha)
            .expect("alpha place should be loaded");

        assert!(beta_index < alpha_index);

        fs::remove_dir_all(root).unwrap();
    }
