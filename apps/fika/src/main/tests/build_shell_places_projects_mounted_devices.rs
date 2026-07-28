
    #[test]
    fn build_shell_places_projects_mounted_devices() {
        let root = test_dir("build-shell-places-devices");
        let places_path = root.join("places.xbel");
        let usb = root.join("USB");
        fs::create_dir_all(&usb).unwrap();
        let devices = vec![
            DeviceInfo {
                id: "mounted-usb".to_string(),
                mount_point: Some(usb.clone()),
                uri: Some("file:///run/media/yk/USB".to_string()),
                filesystem_type: Some("vfat".to_string()),
                label: Some("USB Drive".to_string()),
                capacity_bytes: Some(16 * 1024 * 1024),
                removable: true,
                mounted: true,
                ejectable: true,
                can_power_off: false,
            },
            DeviceInfo {
                id: "unmounted".to_string(),
                mount_point: None,
                uri: Some("file:///dev/sdb1".to_string()),
                filesystem_type: None,
                label: Some("Unmounted".to_string()),
                capacity_bytes: None,
                removable: true,
                mounted: false,
                ejectable: true,
                can_power_off: false,
            },
        ];

        let places = build_shell_places_from_with_devices(&places_path, &devices);

        assert!(places.iter().any(|place| {
            place.group == "Devices"
                && place.marker == "D"
                && place.icon_name == "drive-removable-media"
                && place.label == "USB Drive"
                && place.path == usb
                && place.device.as_ref().is_some_and(|device| device.mounted)
        }));
        assert!(places.iter().any(|place| {
            place.group == "Devices"
                && place.marker == "D"
                && place.icon_name == "drive-removable-media"
                && place.label == "Unmounted"
                && place.path == Path::new("unmounted")
                && place
                    .device
                    .as_ref()
                    .is_some_and(|device| !device.mounted && device.ejectable)
        }));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn shell_places_use_semantic_theme_icon_names() {
        assert_eq!(
            ShellPlace::new("", "H", "Home", PathBuf::from("/tmp/home"), false).icon_name,
            "user-home"
        );
        assert_eq!(
            ShellPlace::new(
                "",
                "Down",
                "Downloads",
                PathBuf::from("/tmp/Downloads"),
                false
            )
            .icon_name,
            "folder-download"
        );
        assert_eq!(
            ShellPlace::new("", "Tr", "Trash", file_ops::trash_files_dir(), false).icon_name,
            "user-trash"
        );
        assert_eq!(
            ShellPlace::new("Network", "Net", "Network", network_root_path(), false).icon_name,
            "folder-remote"
        );
        assert_eq!(
            ShellPlace::new("Devices", "/", "Root", PathBuf::from("/"), false).icon_name,
            "drive-harddisk"
        );
        assert_eq!(
            ShellPlace::new("", "B", "Project", PathBuf::from("/tmp/project"), true).icon_name,
            "folder-bookmark"
        );
    }

    #[test]
    fn places_trash_full_indicator_uses_cached_state() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        let trash = ShellPlace::new("", "Tr", "Trash", file_ops::trash_files_dir(), false);
        let home = ShellPlace::new("", "H", "Home", PathBuf::from("/tmp/home"), false);

        assert!(!scene.trash_place_has_items(&trash));
        scene.trash_has_items = true;
        assert!(scene.trash_place_has_items(&trash));
        assert!(!scene.trash_place_has_items(&home));
    }

    #[test]
    fn device_place_context_menu_uses_mount_and_eject_actions() {
        let mounted = ShellContextTarget::Place {
            index: 0,
            label: "USB Drive".to_string(),
            path: PathBuf::from("/run/media/USB"),
            group: "Devices",
            device: Some(ShellDevicePlace {
                id: "mounted-usb".to_string(),
                mounted: true,
                ejectable: true,
                can_power_off: true,
            }),
            network: false,
            trash: false,
            root: false,
            editable: false,
        };
        assert_eq!(
            context_menu_actions(&mounted),
            &[
                ShellContextMenuAction::OpenInNewPane,
                ShellContextMenuAction::CopyLocation,
                ShellContextMenuAction::UnmountDevice,
                ShellContextMenuAction::EjectDevice,
                ShellContextMenuAction::SafelyRemoveDevice,
                ShellContextMenuAction::Properties,
            ]
        );

        let unmounted = ShellContextTarget::Place {
            index: 1,
            label: "USB Drive".to_string(),
            path: PathBuf::from("gio:volume:usb"),
            group: "Devices",
            device: Some(ShellDevicePlace {
                id: "gio:volume:usb".to_string(),
                mounted: false,
                ejectable: true,
                can_power_off: false,
            }),
            network: false,
            trash: false,
            root: false,
            editable: false,
        };
        assert_eq!(
            context_menu_actions(&unmounted),
            &[
                ShellContextMenuAction::MountDevice,
                ShellContextMenuAction::EjectDevice,
                ShellContextMenuAction::Properties,
            ]
        );
    }

    #[test]
    fn context_target_device_action_preserves_device_id() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.context_target = Some(ShellContextTarget::Place {
            index: 1,
            label: "USB Drive".to_string(),
            path: PathBuf::from("gio:volume:usb"),
            group: "Devices",
            device: Some(ShellDevicePlace {
                id: "gio:volume:usb".to_string(),
                mounted: false,
                ejectable: true,
                can_power_off: false,
            }),
            network: false,
            trash: false,
            root: false,
            editable: false,
        });

        assert_eq!(
            scene.context_target_device_action(ShellContextMenuAction::MountDevice),
            Some(DeviceActionRequest {
                id: "gio:volume:usb".to_string(),
                label: "USB Drive".to_string(),
                action: ShellContextMenuAction::MountDevice,
                operation: DevicePlaceOperation::Mount,
                pane: ShellPaneId::SLOT_0,
                path: PathBuf::from("gio:volume:usb"),
            })
        );
    }

    #[test]
    fn unmounted_device_place_activation_requests_mount() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.places = vec![
            ShellPlace::new("", "H", "Home", PathBuf::from("/tmp/home"), false),
            ShellPlace::new(
                "Devices",
                "D",
                "USB Drive",
                PathBuf::from("gio:volume:usb"),
                false,
            )
            .with_device(ShellDevicePlace {
                id: "gio:volume:usb".to_string(),
                mounted: false,
                ejectable: true,
                can_power_off: false,
            }),
        ];
        let size = PhysicalSize::new(700, 320);
        let row = scene.place_row_rects(size)[1].1;
        let point = ViewPoint {
            x: row.x + 6.0,
            y: row.y + 6.0,
        };

        assert_eq!(
            scene.place_activation_for_press(point, size),
            Some(ShellPlaceActivation::DeviceAction(DeviceActionRequest {
                id: "gio:volume:usb".to_string(),
                label: "USB Drive".to_string(),
                action: ShellContextMenuAction::MountDevice,
                operation: DevicePlaceOperation::Mount,
                pane: ShellPaneId::SLOT_0,
                path: PathBuf::from("gio:volume:usb"),
            }))
        );
        assert_eq!(scene.places_changes, 1);
    }

    #[test]
    fn shell_reader_and_parent_navigation_support_network_paths() {
        let entries = read_shell_entries_sync(&network_root_path()).unwrap();
        assert!(entries.iter().all(|entry| entry.is_dir));

        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.panes[ShellPaneId::SLOT_0].path =
            PathBuf::from("smb://server.example/share/folder/child/");
        assert_eq!(
            scene.parent_directory_path_for_pane(ShellPaneId::SLOT_0),
            Some(PathBuf::from("smb://server.example/share/folder/"))
        );
    }

    #[test]
    fn applying_device_operation_refreshes_places_and_leaves_unmounted_path() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 360);
        let device_path = PathBuf::from("/run/media/usb");
        scene.panes[ShellPaneId::SLOT_0].path = device_path.join("folder");
        let request = DeviceActionRequest {
            id: "gio:mount:usb".to_string(),
            label: "USB".to_string(),
            action: ShellContextMenuAction::UnmountDevice,
            operation: DevicePlaceOperation::Unmount,
            pane: ShellPaneId::SLOT_0,
            path: device_path,
        };
        let result = DevicePlaceOperationResult {
            pane_id: WGPU_SHELL_PANE_ID,
            device_id: request.id.clone(),
            label: request.label.clone(),
            operation: request.operation,
            result: Ok(None),
        };

        scene
            .apply_device_place_operation_result(&request, &result, size)
            .unwrap();

        assert_eq!(scene.places_changes, 1);
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].path, home_dir());
        assert!(scene.context_target.is_none());
        assert!(scene.context_menu.is_none());
    }

    #[test]
    fn add_user_place_at_path_updates_xbel_and_rejects_duplicates() {
        let root = test_dir("add-user-place");
        let places_path = root.join("places.xbel");
        let target = root.join("project");
        fs::create_dir_all(&target).unwrap();

        assert!(add_user_place_at_path(&places_path, &target, "Project".to_string()).unwrap());
        assert_eq!(
            load_user_places(&places_path).unwrap(),
            vec![UserPlace::new("Project".to_string(), target.clone())]
        );
        assert!(!add_user_place_at_path(&places_path, &target, "Again".to_string()).unwrap());
        assert_eq!(
            load_user_places(&places_path).unwrap(),
            vec![UserPlace::new("Project".to_string(), target.clone())]
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn add_context_target_to_places_reloads_places_and_persists_order() {
        let root = test_dir("add-context-place");
        let places_path = root.join("places.xbel");
        let project = root.join("project");
        fs::create_dir_all(&project).unwrap();
        let size = PhysicalSize::new(700, 320);
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.context_target = Some(ShellContextTarget::Blank {
            pane: ShellPaneId::SLOT_0,
            path: project.clone(),
        });
        scene.context_menu = Some(ShellContextMenu::new(
            scene.context_target.clone().unwrap(),
            ViewPoint { x: 8.0, y: 8.0 },
        ));
        scene.properties_overlay = Some(ShellPropertiesOverlay {
            title: "stale".to_string(),
            rows: Vec::new(),
        });

        assert!(
            scene
                .add_context_target_to_places(&places_path, size)
                .unwrap()
        );
        assert!(scene.places.iter().any(|place| place.path == project));
        assert!(scene.context_target.is_none());
        assert!(scene.context_menu.is_none());
        assert!(scene.properties_overlay.is_none());
        assert_eq!(scene.places_changes, 1);
        assert_eq!(
            load_user_places(&places_path).unwrap(),
            vec![UserPlace::new("project".to_string(), project.clone())]
        );
        assert!(
            load_place_order(&place_order_path_for_user_places_path(&places_path))
                .unwrap()
                .iter()
                .any(|path| path == &project)
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn remove_user_place_at_path_updates_xbel_and_order() {
        let root = test_dir("remove-user-place");
        let places_path = root.join("places.xbel");
        let keep = PathBuf::from("/tmp/keep-place");
        let remove = PathBuf::from("/tmp/remove-place");
        save_user_places(
            &places_path,
            &[
                UserPlace::new("Keep".to_string(), keep.clone()),
                UserPlace::new("Remove".to_string(), remove.clone()),
            ],
        )
        .unwrap();
        let order_path = place_order_path_for_user_places_path(&places_path);
        save_place_order(&order_path, &[remove.clone(), keep.clone()]).unwrap();

        assert!(remove_user_place_at_path(&places_path, &remove).unwrap());
        assert_eq!(
            load_user_places(&places_path).unwrap(),
            vec![UserPlace::new("Keep".to_string(), keep.clone())]
        );
        assert_eq!(load_place_order(&order_path).unwrap(), vec![keep]);
        assert!(!remove_user_place_at_path(&places_path, &remove).unwrap());

        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn remove_context_place_reloads_places_and_clears_context_state() {
        let root = test_dir("remove-context-place");
        let places_path = root.join("places.xbel");
        let remove = PathBuf::from("/tmp/remove-context-place-target");
        save_user_places(
            &places_path,
            &[UserPlace::new("Remove Me".to_string(), remove.clone())],
        )
        .unwrap();
        let size = PhysicalSize::new(700, 320);
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.places = build_shell_places_from(&places_path);
        assert!(scene.places.iter().any(|place| place.path == remove));
        scene.context_target = Some(ShellContextTarget::Place {
            index: 1,
            label: "Remove Me".to_string(),
            path: remove.clone(),
            group: "",
            device: None,
            network: false,
            trash: false,
            root: false,
            editable: true,
        });
        scene.context_menu = Some(ShellContextMenu::new(
            scene.context_target.clone().unwrap(),
            ViewPoint { x: 8.0, y: 8.0 },
        ));
        scene.properties_overlay = Some(ShellPropertiesOverlay {
            title: "stale".to_string(),
            rows: Vec::new(),
        });

        assert!(scene.remove_context_place(&places_path, size).unwrap());
        assert!(!scene.places.iter().any(|place| place.path == remove));
        assert!(scene.context_target.is_none());
        assert!(scene.context_menu.is_none());
        assert!(scene.properties_overlay.is_none());
        assert_eq!(scene.places_changes, 1);
        assert_eq!(
            load_user_places(&places_path).unwrap(),
            Vec::<UserPlace>::new()
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn selection_click_supports_single_toggle_and_range() {
        let mut selection = ShellSelection::default();

        assert!(selection.apply_click(Some(2), false, false));
        assert!(selection.contains(2));
        assert_eq!(selection.anchor, Some(2));

        assert!(selection.apply_click(Some(4), false, true));
        assert!(selection.contains(2));
        assert!(selection.contains(4));
        assert_eq!(selection.anchor, Some(4));

        assert!(selection.apply_click(Some(1), true, false));
        assert_eq!(
            selection.selected.iter().copied().collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
        assert_eq!(selection.anchor, Some(4));

        assert!(selection.apply_click(None, false, false));
        assert_eq!(selection.len(), 0);
        assert_eq!(selection.anchor, None);
    }

    #[test]
    fn selection_commands_select_all_and_clear_scene_state() {
        let mut scene = test_scene(
            vec![
                test_entry("alpha.txt", false),
                test_entry("bravo.txt", false),
                test_entry("charlie.txt", false),
            ],
            ShellViewMode::Icons,
        );

        assert!(scene.apply_selection_command(SelectionCommand::SelectAll));
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].selection.len(), 3);
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].selection.anchor, Some(0));
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].selection.focus, Some(2));
        assert_eq!(scene.selection_changes, 1);
        assert!(!scene.apply_selection_command(SelectionCommand::SelectAll));
        assert_eq!(scene.selection_changes, 1);

        scene.rubber_band = Some(RubberBand::new(
            ViewPoint { x: 0.0, y: 0.0 },
            RubberBandMode::Replace,
            scene.panes[ShellPaneId::SLOT_0].selection.clone(),
        ));
        assert!(scene.apply_selection_command(SelectionCommand::Clear));
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].selection.len(), 0);
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].selection.anchor, None);
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].selection.focus, None);
        assert!(scene.rubber_band.is_none());
        assert_eq!(scene.selection_changes, 2);
        assert!(!scene.apply_selection_command(SelectionCommand::Clear));
    }
