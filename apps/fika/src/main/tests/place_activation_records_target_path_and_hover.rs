
    #[test]
    fn place_activation_records_target_path_and_hover() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.places = vec![
            ShellPlace::new("", "H", "Home", PathBuf::from("/tmp"), false),
            ShellPlace::new("", "P", "Projects", PathBuf::from("/tmp/projects"), true),
        ];
        let size = PhysicalSize::new(700, 320);
        let projects_row = scene.place_row_rects(size)[1].1;
        let point = ViewPoint {
            x: projects_row.x + 6.0,
            y: projects_row.y + 6.0,
        };

        assert_eq!(
            scene.place_activation_for_press(point, size),
            Some(ShellPlaceActivation::Open {
                pane: ShellPaneId::SLOT_0,
                path: PathBuf::from("/tmp/projects")
            })
        );
        assert_eq!(scene.hovered_place, Some(1));
        assert_eq!(scene.hovered_item, None);
        assert_eq!(scene.places_changes, 1);
    }

    #[test]
    fn active_place_prefers_longest_matching_prefix() {
        let places = vec![
            ShellPlace::new("Devices", "/", "Root", PathBuf::from("/"), false),
            ShellPlace::new("", "H", "Home", PathBuf::from("/home/yk"), false),
            ShellPlace::new("", "D", "Docs", PathBuf::from("/home/yk/Documents"), true),
        ];

        assert_eq!(
            active_shell_place_index(&places, Path::new("/home/yk/Documents/fika")),
            Some(2)
        );
        assert_eq!(
            active_shell_place_index(&places, Path::new("/home/yk/Code")),
            Some(1)
        );
        assert_eq!(
            active_shell_place_index(&places, Path::new("/etc")),
            Some(0)
        );
    }

    #[test]
    fn places_context_menu_opens_row_actions_without_selecting_items() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        scene.panes[ShellPaneId::SLOT_0]
            .selection
            .apply_navigation(0, false);
        let size = PhysicalSize::new(700, 320);
        let root_row = scene.place_row_rects(size)[1].1;
        let point = ViewPoint {
            x: root_row.x + 5.0,
            y: root_row.y + 5.0,
        };

        assert!(scene.open_context_menu(point, size));
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].selection.len(), 1);
        assert_eq!(scene.hovered_place, Some(1));
        assert_eq!(scene.hovered_item, None);
        assert_eq!(scene.context_target_changes, 1);
        assert_eq!(
            scene.context_target,
            Some(ShellContextTarget::Place {
                index: 1,
                label: "Root".to_string(),
                path: PathBuf::from("/"),
                group: "Devices",
                device: None,
                network: false,
                trash: false,
                root: true,
                editable: false,
            })
        );

        let menu = scene.context_menu.as_ref().expect("menu should open");
        assert_eq!(
            context_menu_actions(&menu.target),
            &[
                ShellContextMenuAction::OpenInNewPane,
                ShellContextMenuAction::CopyLocation,
                ShellContextMenuAction::Properties,
            ]
        );

        let rect = context_menu_rect(menu, size);
        let copy_location_row = ViewPoint {
            x: rect.x + 8.0,
            y: rect.y + CONTEXT_MENU_ROW_HEIGHT + 8.0,
        };
        assert_eq!(
            scene.activate_or_close_context_menu(copy_location_row, size),
            Some(ShellContextMenuAction::CopyLocation)
        );
        assert_eq!(scene.context_menu_actions, 1);
        assert!(scene.context_menu.is_none());
    }

    #[test]
    fn editable_places_context_menu_includes_remove_action() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.places = vec![
            ShellPlace::new("", "H", "Home", PathBuf::from("/tmp"), false),
            ShellPlace::new("", "B", "Project", PathBuf::from("/tmp/project"), true),
        ];
        let size = PhysicalSize::new(700, 320);
        let project_row = scene.place_row_rects(size)[1].1;
        let point = ViewPoint {
            x: project_row.x + 6.0,
            y: project_row.y + 6.0,
        };

        assert!(scene.open_context_menu(point, size));
        let menu = scene.context_menu.as_ref().expect("menu should open");
        assert_eq!(
            context_menu_actions(&menu.target),
            &[
                ShellContextMenuAction::OpenInNewPane,
                ShellContextMenuAction::CopyLocation,
                ShellContextMenuAction::RemovePlace,
                ShellContextMenuAction::Properties,
            ]
        );
        let rect = context_menu_rect(menu, size);
        let remove_row = ViewPoint {
            x: rect.x + 8.0,
            y: rect.y + CONTEXT_MENU_ROW_HEIGHT * 2.0 + 8.0,
        };
        assert_eq!(
            scene.activate_or_close_context_menu(remove_row, size),
            Some(ShellContextMenuAction::RemovePlace)
        );
    }

    #[test]
    fn directory_context_menu_includes_add_to_places_action() {
        let folder_target = ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: PathBuf::from("/tmp/folder"),
            is_dir: true,
            selection_count: 1,
        };
        assert!(
            context_menu_actions(&folder_target).contains(&ShellContextMenuAction::AddToPlaces)
        );
        assert!(context_menu_actions(&folder_target).contains(&ShellContextMenuAction::OpenWith));

        let file_target = ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: PathBuf::from("/tmp/plain.txt"),
            is_dir: false,
            selection_count: 1,
        };
        assert!(!context_menu_actions(&file_target).contains(&ShellContextMenuAction::AddToPlaces));
        assert_eq!(
            context_menu_actions(&file_target).first(),
            Some(&ShellContextMenuAction::OpenWith)
        );

        let blank_target = ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_0,
            path: PathBuf::from("/tmp"),
        };
        assert!(context_menu_actions(&blank_target).contains(&ShellContextMenuAction::AddToPlaces));
        assert!(context_menu_actions(&blank_target).contains(&ShellContextMenuAction::OpenWith));
        assert!(
            context_menu_actions(&blank_target)
                .contains(&ShellContextMenuAction::ToggleHiddenFiles)
        );
        assert!(context_menu_actions(&blank_target).contains(&ShellContextMenuAction::SplitPane));
        assert_eq!(
            ShellContextMenuAction::ToggleHiddenFiles.label_for_hidden_state(false),
            "Show Hidden Files"
        );
        assert_eq!(
            ShellContextMenuAction::ToggleHiddenFiles.label_for_hidden_state(true),
            "Hide Hidden Files"
        );
    }

    #[test]
    fn network_context_menus_hide_unavailable_local_write_actions() {
        let root_blank = ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_0,
            path: network_root_path(),
        };
        assert_eq!(
            context_menu_actions(&root_blank),
            &[
                ShellContextMenuAction::AddNetworkFolder,
                ShellContextMenuAction::SelectAll,
                ShellContextMenuAction::ViewMode,
                ShellContextMenuAction::ToggleHiddenFiles,
                ShellContextMenuAction::SplitPane,
                ShellContextMenuAction::Refresh,
                ShellContextMenuAction::Properties,
            ]
        );

        let remote_blank = ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_0,
            path: PathBuf::from("smb://server/share/folder/"),
        };
        assert_eq!(
            context_menu_actions(&remote_blank),
            &[
                ShellContextMenuAction::AddToPlaces,
                ShellContextMenuAction::SelectAll,
                ShellContextMenuAction::ViewMode,
                ShellContextMenuAction::ToggleHiddenFiles,
                ShellContextMenuAction::SplitPane,
                ShellContextMenuAction::Refresh,
                ShellContextMenuAction::Properties,
            ]
        );
        assert!(!context_menu_actions(&remote_blank).contains(&ShellContextMenuAction::CreateNew));
        assert!(!context_menu_actions(&remote_blank).contains(&ShellContextMenuAction::Paste));
        assert!(
            !context_menu_actions(&remote_blank)
                .contains(&ShellContextMenuAction::PasteAsAdministrator)
        );

        let remote_file = ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: PathBuf::from("sftp://example.test/home/yk/report.txt"),
            is_dir: false,
            selection_count: 1,
        };
        assert_eq!(
            context_menu_actions(&remote_file),
            &[
                ShellContextMenuAction::OpenWith,
                ShellContextMenuAction::CopyLocation,
                ShellContextMenuAction::Properties,
            ]
        );
        assert!(!context_menu_actions(&remote_file).contains(&ShellContextMenuAction::Cut));
        assert!(!context_menu_actions(&remote_file).contains(&ShellContextMenuAction::Rename));
        assert!(!context_menu_actions(&remote_file).contains(&ShellContextMenuAction::MoveToTrash));

        let remote_dir = ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: PathBuf::from("sftp://example.test/home/yk/projects/"),
            is_dir: true,
            selection_count: 1,
        };
        assert_eq!(
            context_menu_actions(&remote_dir),
            &[
                ShellContextMenuAction::OpenInNewPane,
                ShellContextMenuAction::AddToPlaces,
                ShellContextMenuAction::CopyLocation,
                ShellContextMenuAction::Properties,
            ]
        );
    }

    #[test]
    fn network_root_place_context_menu_offers_add_network_folder() {
        let target = ShellContextTarget::Place {
            index: 0,
            label: "Network".to_string(),
            path: network_root_path(),
            group: "Network",
            device: None,
            network: true,
            trash: false,
            root: false,
            editable: false,
        };

        assert_eq!(
            context_menu_actions(&target),
            &[
                ShellContextMenuAction::OpenInNewPane,
                ShellContextMenuAction::AddNetworkFolder,
                ShellContextMenuAction::CopyLocation,
                ShellContextMenuAction::Properties,
            ]
        );
        assert_eq!(
            ShellContextMenuAction::AddNetworkFolder.label(),
            "Add Network Folder..."
        );
        assert_eq!(
            ShellContextMenuAction::AddNetworkFolder.as_str(),
            "add-network-folder"
        );
    }

    #[test]
    fn add_network_folder_opens_builtin_uri_input() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.context_target = Some(ShellContextTarget::Place {
            index: 0,
            label: "Network".to_string(),
            path: network_root_path(),
            group: "Network",
            device: None,
            network: true,
            trash: false,
            root: false,
            editable: false,
        });
        scene.context_menu = Some(ShellContextMenu::new(
            scene.context_target.clone().unwrap(),
            ViewPoint { x: 20.0, y: 20.0 },
        ));
        let size = PhysicalSize::new(700, 320);

        assert!(scene.open_add_network_folder_location_draft(size));

        assert_eq!(
            scene.location_draft_purpose(),
            Some(LocationDraftPurpose::AddNetworkFolder)
        );
        assert_eq!(scene.location_draft_value(), Some("smb://"));
        assert!(scene.context_target.is_none());
        assert!(scene.context_menu.is_none());
    }

    #[test]
    fn add_network_folder_request_saves_network_bookmark() {
        let root = test_dir("add-network-folder-bookmark");
        let places_path = root.join("places.xbel");
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        assert!(scene.open_add_network_folder_location_draft(size));
        scene
            .location_draft
            .as_mut()
            .unwrap()
            .draft
            .set_completed("smb://server/Share%20Name/".to_string());

        let request = scene.add_network_folder_request_from_draft().unwrap();
        assert_eq!(request.pane, ShellPaneId::SLOT_0);
        assert_eq!(request.path, PathBuf::from("smb://server/Share%20Name/"));
        assert_eq!(request.label, "Share Name on server");
        assert!(
            scene
                .add_network_folder_place(&places_path, &request.path, request.label.clone(), size)
                .unwrap()
        );

        assert_eq!(
            load_user_places(&places_path).unwrap(),
            vec![UserPlace::new(
                "Share Name on server".to_string(),
                PathBuf::from("smb://server/Share%20Name/")
            )]
        );
        let places = build_shell_places_from(&places_path);
        assert!(places.iter().any(|place| {
            place.group == "Network"
                && place.label == "Share Name on server"
                && place.path == Path::new("smb://server/Share%20Name/")
                && place.network
                && place.editable
        }));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn context_menu_items_offer_open_with_root_applications_and_submenu() {
        let target = ShellContextTarget::Item {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            path: PathBuf::from("/tmp/plain.txt"),
            is_dir: false,
            selection_count: 1,
        };
        let app = |id: &str, name: &str, icon: Option<&str>, is_default: bool| MimeApplication {
            id: format!("org.example.{id}.desktop"),
            desktop_file: PathBuf::from(format!(
                "/usr/share/applications/org.example.{id}.desktop"
            )),
            name: name.to_string(),
            exec: format!("{} %F", name.to_ascii_lowercase()),
            icon: icon.map(str::to_string),
            is_default,
        };
        let menu = ShellContextMenu::with_dynamic(
            target,
            ViewPoint { x: 20.0, y: 20.0 },
            vec![
                app("Editor", "Editor", Some("accessories-text-editor"), true),
                app("Viewer", "Viewer", Some("image-viewer"), false),
                app("Paint", "Paint", Some("applications-graphics"), false),
                app("Notes", "Notes", None, false),
            ],
            Vec::new(),
        );

        let root = context_menu_items(&menu);
        let open_with_index = root
            .iter()
            .position(|item| item.submenu == Some(ShellContextSubmenu::OpenWith))
            .expect("Open With submenu should remain present");
        let root_apps = root
            .iter()
            .filter_map(|item| match &item.command {
                ShellContextMenuCommand::OpenWithApplication { desktop_id } => {
                    Some((item.label.as_str(), desktop_id.as_str(), &item.icon))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(root_apps.len(), 2);
        assert_eq!(
            root_apps
                .iter()
                .map(|(label, desktop_id, _)| (*label, *desktop_id))
                .collect::<Vec<_>>(),
            vec![
                ("Open With Viewer", "org.example.Viewer.desktop"),
                ("Open With Paint", "org.example.Paint.desktop"),
            ]
        );
        assert_eq!(root[open_with_index - 2].label, "Open With Viewer");
        assert_eq!(root[open_with_index - 1].label, "Open With Paint");
        assert!(matches!(
            root_apps.first().map(|(_, _, icon)| *icon),
            Some(ShellContextMenuIcon::Application(Some(icon))) if icon == "image-viewer"
        ));
        let submenu = context_submenu_actions(ShellContextSubmenu::OpenWith, &menu);
        assert!(matches!(
            submenu.first().map(|item| &item.command),
            Some(ShellContextMenuCommand::OpenWithApplication { desktop_id })
                if desktop_id == "org.example.Editor.desktop"
        ));
        assert!(matches!(
            submenu.last().map(|item| &item.command),
            Some(ShellContextMenuCommand::Builtin(
                ShellContextMenuAction::OpenWith
            ))
        ));
    }
