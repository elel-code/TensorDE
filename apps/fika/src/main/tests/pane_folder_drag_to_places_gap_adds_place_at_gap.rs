
    #[test]
    fn pane_folder_drag_to_places_gap_adds_place_at_gap() {
        let root = test_dir("pane-folder-dnd-add-place");
        let places_path = root.join("places.xbel");
        let beta = PathBuf::from("/tmp/dnd-existing-beta");
        save_user_places(
            &places_path,
            &[UserPlace::new("Beta".to_string(), beta.clone())],
        )
        .unwrap();
        let mut scene = test_scene(vec![test_entry("project", true)], ShellViewMode::Icons);
        scene.places = build_shell_places_from(&places_path);
        let size = PhysicalSize::new(900, 760);
        let beta_index = scene
            .places
            .iter()
            .position(|place| place.path == beta)
            .expect("test user place should be visible");
        let gap = scene
            .place_gap_rect_for_index(beta_index, size)
            .expect("gap before beta should be visible");
        let projection = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();
        let item = projection.visible_items[0];
        let start = ViewPoint {
            x: projection.geometry.content.x + item.layout.visual_rect.x + 6.0,
            y: projection.geometry.content.y + item.layout.visual_rect.y + 6.0,
        };
        let gap_point = ViewPoint {
            x: gap.x + gap.width / 2.0,
            y: gap.y + gap.height / 2.0,
        };

        assert!(scene.begin_internal_drag_for_pane_item(ShellPaneId::SLOT_0, 0, start));
        assert!(scene.set_pointer(gap_point, size));
        assert_eq!(
            scene.dnd_hover_target,
            Some(ShellDropTarget::PlacesGap { index: beta_index })
        );
        assert!(
            scene
                .finish_internal_drag_with_user_places_path(gap_point, size, &places_path)
                .unwrap()
        );

        let project = PathBuf::from("/tmp/project");
        let project_index = scene
            .places
            .iter()
            .position(|place| place.path == project)
            .expect("project folder should be added to places");
        let beta_index_after = scene
            .places
            .iter()
            .position(|place| place.path == beta)
            .expect("existing place should remain");
        assert!(project_index < beta_index_after);
        assert!(
            load_user_places(&places_path)
                .unwrap()
                .iter()
                .any(|place| place.path == project)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn local_wayland_offer_keeps_internal_places_gap_semantics() {
        let beta = PathBuf::from("/tmp/dnd-existing-beta");
        let mut scene = test_scene(vec![test_entry("project", true)], ShellViewMode::Icons);
        scene.places = vec![ShellPlace::new("", "B", "Beta", beta, true)];
        let size = PhysicalSize::new(900, 760);
        let gap = scene
            .place_gap_rect_for_index(0, size)
            .expect("gap before beta should be visible");
        let gap_point = ViewPoint {
            x: gap.x + gap.width / 2.0,
            y: gap.y + gap.height / 2.0,
        };
        let path = PathBuf::from("/tmp/project");
        let source = ShellInternalDragSource::PaneItem {
            pane: ShellPaneId::SLOT_0,
            index: 0,
            source_path: path.clone(),
            is_dir: true,
        };

        assert!(scene.begin_data_transfer_drag(
            vec![path],
            Some(source.clone()),
            gap_point,
            size,
        ));
        assert_eq!(
            scene.dnd_hover_target,
            Some(ShellDropTarget::PlacesGap { index: 0 })
        );
        assert_eq!(
            scene
                .external_drag
                .as_ref()
                .and_then(|drag| drag.local_source.as_ref()),
            Some(&source)
        );
        assert!(scene.internal_drag.is_none());
    }

    #[test]
    fn device_and_network_places_do_not_participate_in_internal_dnd() {
        let mut scene = test_scene(vec![test_entry("note.txt", false)], ShellViewMode::Icons);
        scene.places = vec![
            ShellPlace::new("", "H", "Home", PathBuf::from("/tmp/home"), false),
            ShellPlace::new("Network", "Net", "Network", network_root_path(), false),
            ShellPlace::new("Devices", "/", "Root", PathBuf::from("/"), false),
            ShellPlace::new(
                "Devices",
                "D",
                "Disk",
                PathBuf::from("/run/media/disk"),
                false,
            )
            .with_device(ShellDevicePlace {
                id: "disk".to_string(),
                mounted: true,
                ejectable: true,
                can_power_off: false,
            }),
        ];
        let size = PhysicalSize::new(760, 520);

        assert!(!scene.begin_internal_drag_for_place(1, ViewPoint { x: 0.0, y: 0.0 }));
        assert!(!scene.begin_internal_drag_for_place(2, ViewPoint { x: 0.0, y: 0.0 }));
        assert!(!scene.begin_internal_drag_for_place(3, ViewPoint { x: 0.0, y: 0.0 }));

        let network_row = scene.place_row_rects(size)[1].1;
        let network_point = ViewPoint {
            x: network_row.x + 6.0,
            y: network_row.y + 6.0,
        };
        assert_eq!(scene.begin_place_pointer(network_point, size), Some(true));
        assert!(scene.internal_drag.is_none());
        let (_changed, activation) = scene.end_place_pointer(network_point, size);
        assert_eq!(
            activation,
            Some(ShellPlaceActivation::Open {
                pane: ShellPaneId::SLOT_0,
                path: network_root_path()
            })
        );

        let projection = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();
        let item = projection.visible_items[0];
        let start = ViewPoint {
            x: projection.geometry.content.x + item.layout.visual_rect.x + 6.0,
            y: projection.geometry.content.y + item.layout.visual_rect.y + 6.0,
        };
        let device_row = scene.place_row_rects(size)[3].1;
        let device_point = ViewPoint {
            x: device_row.x + device_row.width / 2.0,
            y: device_row.y + device_row.height / 2.0,
        };

        assert!(scene.begin_internal_drag_for_pane_item(ShellPaneId::SLOT_0, 0, start));
        assert!(scene.set_pointer(device_point, size));
        assert_eq!(scene.dnd_hover_target, None);
        assert!(!scene.finish_internal_drag(device_point, size));
        assert!(scene.drop_menu.is_none());
    }

    #[test]
    fn last_places_gap_stays_attached_to_trash_before_network_section() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.places = vec![
            ShellPlace::new("", "H", "Home", PathBuf::from("/tmp/home"), false),
            ShellPlace::new("", "Tr", "Trash", file_ops::trash_files_dir(), false),
            ShellPlace::new("Network", "Net", "Network", network_root_path(), false),
            ShellPlace::new("Devices", "/", "Root", PathBuf::from("/"), false),
        ];
        let size = PhysicalSize::new(760, 520);
        let rows = scene.place_row_rects(size);
        let trash_rect = rows[1].1;
        let network_rect = rows[2].1;
        let last_left_gap = scene
            .place_gap_rect_for_index(2, size)
            .expect("gap after Trash should be visible");

        assert!(
            (last_left_gap.y + last_left_gap.height / 2.0 - trash_rect.bottom()).abs()
                < f32::EPSILON
        );
        assert!(last_left_gap.bottom() < network_rect.y);
        assert!(scene.place_gap_rect_for_index(3, size).is_none());
        assert!(scene.place_gap_rect_for_index(4, size).is_none());
    }

    #[test]
    fn pane_file_drag_to_place_item_opens_drop_menu_for_place_path() {
        let mut scene = test_scene(vec![test_entry("note.txt", false)], ShellViewMode::Icons);
        scene.places = vec![ShellPlace::new(
            "",
            "T",
            "Target",
            PathBuf::from("/tmp/drop-target"),
            true,
        )];
        let size = PhysicalSize::new(700, 360);
        let projection = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();
        let item = projection.visible_items[0];
        let start = ViewPoint {
            x: projection.geometry.content.x + item.layout.visual_rect.x + 6.0,
            y: projection.geometry.content.y + item.layout.visual_rect.y + 6.0,
        };
        let row = scene.place_row_rects(size)[0].1;
        let target = ViewPoint {
            x: row.x + row.width / 2.0,
            y: row.y + row.height / 2.0,
        };

        assert!(scene.begin_internal_drag_for_pane_item(ShellPaneId::SLOT_0, 0, start));
        assert!(scene.set_pointer(target, size));
        assert_eq!(
            scene.dnd_hover_target,
            Some(ShellDropTarget::Place {
                index: 0,
                path: PathBuf::from("/tmp/drop-target")
            })
        );
        assert!(scene.finish_internal_drag(target, size));
        let menu = scene.drop_menu.as_ref().expect("drop menu should open");
        assert_eq!(menu.sources, vec![PathBuf::from("/tmp/note.txt")]);
        assert_eq!(menu.target_dir, PathBuf::from("/tmp/drop-target"));
        assert!(matches!(
            menu.target,
            ShellDropTarget::Place { index: 0, .. }
        ));
    }

    #[test]
    fn place_drag_to_pane_folder_and_blank_opens_drop_menu() {
        let mut scene = test_scene(vec![test_entry("folder", true)], ShellViewMode::Icons);
        scene.places = vec![ShellPlace::new(
            "",
            "S",
            "Source",
            PathBuf::from("/tmp/source-place"),
            true,
        )];
        let size = PhysicalSize::new(700, 360);
        let place_row = scene.place_row_rects(size)[0].1;
        let place_point = ViewPoint {
            x: place_row.x + 6.0,
            y: place_row.y + 6.0,
        };
        let projection = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();
        let content = projection.geometry.content;
        let folder = projection.visible_items[0];
        let folder_point = ViewPoint {
            x: content.x + folder.layout.visual_rect.x + 6.0,
            y: content.y + folder.layout.visual_rect.y + 6.0,
        };

        assert_eq!(scene.begin_place_pointer(place_point, size), Some(true));
        assert!(scene.set_pointer(folder_point, size));
        let (changed, activation) = scene.end_place_pointer(folder_point, size);
        assert!(changed);
        assert!(activation.is_none());
        let menu = scene
            .drop_menu
            .as_ref()
            .expect("folder drop menu should open");
        assert_eq!(menu.sources, vec![PathBuf::from("/tmp/source-place")]);
        assert_eq!(menu.target_dir, PathBuf::from("/tmp/folder"));
        assert!(matches!(
            menu.target,
            ShellDropTarget::PaneItem { is_dir: true, .. }
        ));

        scene.drop_menu = None;
        let blank_point = ViewPoint {
            x: content.right() - 4.0,
            y: content.bottom() - 4.0,
        };
        assert_eq!(scene.begin_place_pointer(place_point, size), Some(true));
        assert!(scene.set_pointer(blank_point, size));
        let (changed, activation) = scene.end_place_pointer(blank_point, size);
        assert!(changed);
        assert!(activation.is_none());
        let menu = scene
            .drop_menu
            .as_ref()
            .expect("blank drop menu should open");
        assert_eq!(menu.sources, vec![PathBuf::from("/tmp/source-place")]);
        assert_eq!(menu.target_dir, PathBuf::from("/tmp"));
        assert!(matches!(menu.target, ShellDropTarget::PaneBlank { .. }));
    }

    #[test]
    fn place_pointer_drag_uses_internal_drag_flow_like_pane_items() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.places = vec![
            ShellPlace::new("", "A", "Alpha", PathBuf::from("/tmp/a"), true),
            ShellPlace::new("", "B", "Beta", PathBuf::from("/tmp/b"), true),
        ];
        let size = PhysicalSize::new(700, 360);
        let source_row = scene.place_row_rects(size)[0].1;
        let start = ViewPoint {
            x: source_row.x + 6.0,
            y: source_row.y + 6.0,
        };

        assert_eq!(scene.begin_place_pointer(start, size), Some(true));
        assert!(scene.place_press.is_none());
        let drag = scene
            .internal_drag
            .as_ref()
            .expect("editable place should use internal drag");
        assert_eq!(drag.source_place_index(), Some(0));
        assert!(!drag.active);

        let (changed, activation) = scene.end_place_pointer(start, size);
        assert!(changed);
        assert_eq!(
            activation,
            Some(ShellPlaceActivation::Open {
                pane: ShellPaneId::SLOT_0,
                path: PathBuf::from("/tmp/a")
            })
        );
        assert!(scene.internal_drag.is_none());

        assert_eq!(scene.begin_place_pointer(start, size), Some(true));
        assert!(scene.place_press.is_none());
        let self_hover = ViewPoint {
            x: source_row.x + source_row.width / 2.0,
            y: source_row.y + source_row.height / 2.0,
        };
        assert!(scene.set_pointer(self_hover, size));
        let drag = scene
            .internal_drag
            .as_ref()
            .expect("place drag should still use internal drag");
        assert!(drag.active);
        assert_eq!(scene.dnd_hover_target, None);

        let (changed, activation) = scene.end_place_pointer(self_hover, size);
        assert!(changed);
        assert!(activation.is_none());
        assert!(scene.internal_drag.is_none());
        assert!(scene.drop_menu.is_none());
    }

    #[test]
    fn place_drag_to_own_adjacent_gaps_has_no_drop_target() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.places = vec![
            ShellPlace::new("", "A", "Alpha", PathBuf::from("/tmp/a"), true),
            ShellPlace::new("", "B", "Beta", PathBuf::from("/tmp/b"), true),
        ];
        let size = PhysicalSize::new(700, 360);
        let row = scene.place_row_rects(size)[0].1;
        let start = ViewPoint {
            x: row.x + 6.0,
            y: row.y + 6.0,
        };
        let own_gap = scene.place_gap_rect_for_index(0, size).unwrap();
        let own_gap_point = ViewPoint {
            x: own_gap.x + own_gap.width / 2.0,
            y: own_gap.y + own_gap.height / 2.0,
        };
        assert!(scene.begin_internal_drag_for_place(0, start));
        // First move activates the drag (threshold); may return true.
        assert!(scene.set_pointer(own_gap_point, size));
        assert_eq!(scene.dnd_hover_target, None);

        let after_gap = scene.place_gap_rect_for_index(1, size).unwrap();
        let after_gap_point = ViewPoint {
            x: after_gap.x + after_gap.width / 2.0,
            y: after_gap.y + after_gap.height / 2.0,
        };
        // Adjacent self-gaps stay without a drop target; motion alone does not
        // dirty the window once the drag is already active.
        let _ = scene.set_pointer(after_gap_point, size);
        assert_eq!(scene.dnd_hover_target, None);
    }

    #[test]
    fn place_drag_over_source_row_keeps_preview_without_drop_target() {
        let mut scene = test_scene(Vec::new(), ShellViewMode::Icons);
        scene.places = vec![
            ShellPlace::new("", "A", "Alpha", PathBuf::from("/tmp/a"), true),
            ShellPlace::new("", "B", "Beta", PathBuf::from("/tmp/b"), true),
        ];
        let size = PhysicalSize::new(700, 360);
        let source_row = scene.place_row_rects(size)[0].1;
        let start = ViewPoint {
            x: source_row.x + 6.0,
            y: source_row.y + 6.0,
        };
        let self_hover = ViewPoint {
            x: source_row.x + source_row.width / 2.0,
            y: source_row.y + source_row.height / 2.0,
        };

        assert!(scene.begin_internal_drag_for_place(0, start));
        // Motion over the source row does not set a drop target and need not
        // request a window redraw (preview is a Wayland DnD icon).
        let _ = scene.set_pointer(self_hover, size);
        let drag = scene
            .internal_drag
            .as_ref()
            .expect("place drag should exist");
        assert!(drag.active);
        assert_eq!(scene.active_place_drag_source_index(), Some(0));
        assert_eq!(scene.hovered_place, None);
        assert_eq!(scene.dnd_hover_target, None);

        let valid_gap = scene.place_row_rects(size)[1].1;
        let valid_gap_point = ViewPoint {
            x: valid_gap.x + valid_gap.width / 2.0,
            y: valid_gap.y + valid_gap.height * 0.75,
        };
        assert!(scene.set_pointer(valid_gap_point, size));
        let drag = scene
            .internal_drag
            .as_ref()
            .expect("place drag should continue");
        assert!(drag.active);
        assert_eq!(
            scene.dnd_hover_target,
            Some(ShellDropTarget::PlacesGap { index: 2 })
        );
    }


