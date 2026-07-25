    fn test_entry(name: &str, is_dir: bool) -> Entry {
        test_entry_with_mime(
            name,
            is_dir,
            if is_dir {
                "inode/directory"
            } else {
                "text/plain"
            },
        )
    }

    fn test_entry_with_mime(name: &str, is_dir: bool, mime_type: &'static str) -> Entry {
        test_entry_with_mime_and_modified(name, is_dir, mime_type, None)
    }

    fn test_entry_with_mime_and_modified(
        name: &str,
        is_dir: bool,
        mime_type: &'static str,
        modified_secs: Option<u64>,
    ) -> Entry {
        Entry::new(fika_core::EntryData {
            name: Arc::from(name),
            name_width_units: name.len() as u16,
            target_path: None,
            size_bytes: 0,
            modified_secs,
            metadata_complete: true,
            mime_type: Some(Arc::from(mime_type)),
            mime_magic_checked: true,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir,
        })
    }

    fn test_entry_with_target(name: &str, is_dir: bool, target_path: PathBuf) -> Entry {
        Entry::new(fika_core::EntryData {
            name: Arc::from(name),
            name_width_units: name.len() as u16,
            target_path: Some(target_path),
            size_bytes: 0,
            modified_secs: None,
            metadata_complete: true,
            mime_type: Some(Arc::from(if is_dir {
                "inode/directory"
            } else {
                "text/plain"
            })),
            mime_magic_checked: true,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir,
        })
    }

    fn test_unchecked_generic_entry(name: &str, size_bytes: u64, modified_secs: u64) -> Entry {
        Entry::new(fika_core::EntryData {
            name: Arc::from(name),
            name_width_units: name.len() as u16,
            target_path: None,
            size_bytes,
            modified_secs: Some(modified_secs),
            metadata_complete: true,
            mime_type: Some(Arc::from(fika_core::GENERIC_BINARY_MIME)),
            mime_magic_checked: false,
            trash_original_path: None,
            trash_deletion_time: None,
            is_dir: false,
        })
    }

    fn test_dir(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("fika-wgpu-{name}-{unique}"))
    }

    #[test]
    fn view_mode_setting_round_trips_and_startup_uses_storage() {
        let root = test_dir("view-mode-settings");
        let settings_path = root.join("storage/settings.tsv");
        let settings = fika_core::AppSettings {
            places_sidebar: fika_core::PlacesSidebarSettings {
                width: Some(288.0),
                visible: Some(true),
            },
            view: fika_core::ViewSettings::default(),
            appearance: fika_core::AppearanceSettings::default(),
        };
        save_app_settings(&settings_path, &settings).unwrap();

        save_view_mode_setting(&settings_path, ShellViewMode::Details).unwrap();
        save_show_hidden_setting(&settings_path, true).unwrap();
        save_dark_mode_setting(&settings_path, true).unwrap();
        save_background_effect_settings(&settings_path, true, 0.78).unwrap();
        let loaded = load_app_settings(&settings_path).unwrap();
        assert_eq!(loaded.places_sidebar.width, Some(288.0));
        assert_eq!(loaded.places_sidebar.visible, Some(true));
        assert_eq!(loaded.view.mode, Some(ShellViewMode::Details));
        assert_eq!(loaded.view.show_hidden, Some(true));
        assert_eq!(loaded.appearance.dark_mode, Some(true));
        assert_eq!(loaded.appearance.background_blur, Some(true));
        assert_eq!(loaded.appearance.background_opacity, Some(0.78));
        assert_eq!(
            startup_view_mode(ShellViewMode::Icons, false, &loaded),
            ShellViewMode::Details
        );
        assert_eq!(
            startup_view_mode(ShellViewMode::Compact, true, &loaded),
            ShellViewMode::Compact
        );
        assert!(startup_show_hidden(&loaded));
        assert!(startup_places_visible(&loaded));
        assert!(startup_dark_mode(&loaded));
        assert!(startup_background_blur(&loaded));
        assert_eq!(startup_background_opacity(&loaded), 0.8);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn startup_hidden_visibility_applies_to_initial_pane() {
        let root = test_dir("startup-hidden-files");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("visible.txt"), b"visible").unwrap();
        fs::write(root.join(".hidden.txt"), b"hidden").unwrap();

        let hidden =
            ShellScene::load_with_hidden_visibility(root.clone(), ShellViewMode::Icons, true)
                .unwrap();
        assert!(hidden.show_hidden);
        assert_eq!(hidden.panes[ShellPaneId::SLOT_0].filtered_indexes.len(), 2);

        let visible_only =
            ShellScene::load_with_hidden_visibility(root.clone(), ShellViewMode::Icons, false)
                .unwrap();
        assert!(!visible_only.show_hidden);
        assert_eq!(
            filtered_names(&visible_only, ShellPaneId::SLOT_0),
            vec!["visible.txt"]
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reload_after_delete_animates_surviving_item_reflow() {
        let root = test_dir("delete-reflow-animation");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("alpha.txt"), b"a").unwrap();
        fs::write(root.join("beta.txt"), b"b").unwrap();
        fs::write(root.join("gamma.txt"), b"g").unwrap();

        let mut scene = ShellScene::load(root.clone(), ShellViewMode::Icons).unwrap();
        let size = PhysicalSize::new(720, 360);
        fs::remove_file(root.join("beta.txt")).unwrap();

        assert!(scene.reload_current_path(size).unwrap());
        let gamma = root.join("gamma.txt");
        let transition = scene
            .animations
            .item_reflow_transitions()
            .iter()
            .find(|transition| transition.path == gamma)
            .expect("surviving item after deleted entry should reflow");

        assert_eq!(transition.pane, ShellPaneId::SLOT_0);
        assert!(transition.moved());
        assert!(scene.animation_active());
        assert_ne!(scene.animation_dirty_value_with_hover(true), 0);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn window_resize_animates_visible_item_reflow() {
        let mut scene = test_scene(
            (0..8)
                .map(|index| test_entry(&format!("item-{index:02}.txt"), false))
                .collect(),
            ShellViewMode::Icons,
        );
        let narrow = PhysicalSize::new(520, 360);
        let wide = PhysicalSize::new(800, 360);
        let narrow_columns = match scene.layout(narrow) {
            ShellLayout::Icons(layout) => layout.columns_per_row(),
            _ => unreachable!(),
        };
        let wide_columns = match scene.layout(wide) {
            ShellLayout::Icons(layout) => layout.columns_per_row(),
            _ => unreachable!(),
        };
        assert!(narrow_columns < wide_columns);

        assert!(scene.reflow_pane_items_after_window_resize(narrow, wide));
        assert!(shell::item_reflow::has_pending_item_reflow(&scene));
        assert!(scene.animations.item_reflow_transitions().is_empty());
        let target = PathBuf::from("/tmp/item-02.txt");
        let previous_rect = scene
            .visible_item_rects_by_path_for_pane(ShellPaneId::SLOT_0, narrow)
            .remove(&target)
            .expect("target should be visible before resize");
        let next_rect = scene
            .visible_item_rects_by_path_for_pane(ShellPaneId::SLOT_0, wide)
            .remove(&target)
            .expect("target should remain visible after resize");
        assert_eq!(
            scene.item_reflow_offset_for_path(ShellPaneId::SLOT_0, &target),
            Some((previous_rect.x - next_rect.x, previous_rect.y - next_rect.y))
        );

        assert!(shell::item_reflow::start_due_item_reflow_transitions(
            &mut scene,
            Instant::now() + ITEM_REFLOW_ANIMATION_DELAY + Duration::from_millis(1)
        ));
        let transition = scene
            .animations
            .item_reflow_transitions()
            .iter()
            .find(|transition| transition.path == target)
            .expect("item should reflow when resize changes icon columns");

        assert_eq!(transition.pane, ShellPaneId::SLOT_0);
        assert_eq!(transition.to, next_rect);
        assert!(transition.moved());
        assert!(scene.animation_active());
    }

    #[test]
    fn window_resize_height_only_does_not_animate_item_reflow() {
        let mut scene = test_scene(
            (0..8)
                .map(|index| test_entry(&format!("item-{index:02}.txt"), false))
                .collect(),
            ShellViewMode::Icons,
        );
        let short = PhysicalSize::new(720, 320);
        let tall = PhysicalSize::new(720, 460);

        assert!(!scene.reflow_pane_items_after_window_resize(short, tall));
        assert!(!shell::item_reflow::has_pending_item_reflow(&scene));
        assert!(scene.animations.item_reflow_transitions().is_empty());
    }

    #[test]
    fn shell_metadata_candidate_targets_unchecked_generic_file() {
        let entry = test_unchecked_generic_entry("payload", 12, 42);
        let candidate = shell_metadata_role_candidate(Path::new("/tmp/fika-metadata"), 3, &entry)
            .expect("unchecked generic file should require MIME magic metadata");

        assert_eq!(candidate.item_id, shell_metadata_item_id(3));
        assert_eq!(candidate.path, PathBuf::from("/tmp/fika-metadata/payload"));
        assert_eq!(candidate.size_bytes, 12);
        assert_eq!(candidate.modified_secs, Some(42));
        assert_eq!(
            candidate.mime_type.as_deref(),
            Some(fika_core::GENERIC_BINARY_MIME)
        );

        let checked = test_entry_with_mime("plain.txt", false, "text/plain");
        assert!(shell_metadata_role_candidate(Path::new("/tmp"), 0, &checked).is_none());
    }

    #[test]
    fn shell_metadata_result_updates_matching_entry_only() {
        let mut scene = test_scene(
            vec![test_unchecked_generic_entry("payload", 12, 42)],
            ShellViewMode::Icons,
        );
        let stale = MetadataRoleResult {
            pane_id: core_pane_id_for_shell_pane(ShellPaneId::SLOT_0),
            generation: Generation(0),
            item_id: shell_metadata_item_id(0),
            path: PathBuf::from("/tmp/other"),
            role: Some(fika_core::EntryMetadataRole {
                size_bytes: 12,
                modified_secs: Some(42),
                mime_type: Some(Arc::from("image/png")),
                mime_magic_checked: true,
            }),
        };
        assert!(!scene.apply_metadata_role_result(stale));
        assert!(!scene.panes[ShellPaneId::SLOT_0].entries[0].mime_magic_checked);

        let matching = MetadataRoleResult {
            pane_id: core_pane_id_for_shell_pane(ShellPaneId::SLOT_0),
            generation: Generation(0),
            item_id: shell_metadata_item_id(0),
            path: PathBuf::from("/tmp/payload"),
            role: Some(fika_core::EntryMetadataRole {
                size_bytes: 12,
                modified_secs: Some(42),
                mime_type: Some(Arc::from("image/png")),
                mime_magic_checked: true,
            }),
        };
        assert!(scene.apply_metadata_role_result(matching));
        let entry = &scene.panes[ShellPaneId::SLOT_0].entries[0];
        assert!(entry.mime_magic_checked);
        assert_eq!(entry.mime_type.as_deref(), Some("image/png"));
    }

    fn test_desktop_application(
        id: &str,
        name: &str,
        exec: &str,
        mime_types: &[&str],
    ) -> fika_core::DesktopApplication {
        fika_core::DesktopApplication {
            id: id.to_string(),
            desktop_file: PathBuf::from(format!("/apps/{id}")),
            name: name.to_string(),
            exec: exec.to_string(),
            icon: None,
            categories: Vec::new(),
            mime_types: mime_types.iter().map(|mime| mime.to_string()).collect(),
            actions: Vec::new(),
        }
    }

    /// In-memory scene fixture with deterministic places (no disk listing).
    ///
    /// Defaults live in [`ShellScene::from_primary_pane`]; prefer this over
    /// hand-writing `ShellScene { ... }` when adding fields.
    fn test_scene(entries: Vec<Entry>, view_mode: ShellViewMode) -> ShellScene {
        test_scene_at(PathBuf::from("/tmp"), entries, view_mode)
    }

    fn test_scene_at(
        path: PathBuf,
        entries: Vec<Entry>,
        view_mode: ShellViewMode,
    ) -> ShellScene {
        ShellScene::from_primary_pane(
            ShellPaneState::from_entries(path.clone(), view_mode, entries, false, ""),
            vec![
                ShellPlace::new("", "H", "Home", path, false),
                ShellPlace::new("Devices", "/", "Root", PathBuf::from("/"), false),
            ],
            false,
            false,
        )
    }

    fn set_test_pane(
        scene: &mut ShellScene,
        pane: ShellPaneId,
        path: PathBuf,
        view_mode: ShellViewMode,
        entries: Vec<Entry>,
    ) {
        let dir_count = entries.iter().filter(|entry| entry.is_dir).count();
        let filtered_indexes = filtered_indexes_for_entries(
            &entries,
            scene.show_hidden,
            scene.filter_pattern_for_pane(pane),
        );
        scene.panes.set(
            pane,
            ShellPaneState {
                path,
                view_mode,
                zoom_step: 0,
                dir_count,
                filtered_indexes,
                entries,
                selection: ShellSelection::default(),
                scroll_x: 0.0,
                scroll_y: 0.0,
            },
        );
    }

    fn filtered_names(scene: &ShellScene, pane: ShellPaneId) -> Vec<String> {
        let pane = scene.pane_state(pane).expect("test pane should be open");
        pane.filtered_indexes
            .iter()
            .map(|index| pane.entries[*index].name.as_ref().to_string())
            .collect()
    }

    #[test]
    fn places_hit_testing_is_separate_from_file_content() {
        let mut scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(700, 320);
        let place_row = scene.place_row_rects(size)[0].1;
        let place_point = ViewPoint {
            x: place_row.x + 4.0,
            y: place_row.y + 4.0,
        };

        assert_eq!(
            scene.place_index_at_screen_point(place_point, size),
            Some(0)
        );
        assert_eq!(scene.hit_test_screen_point(place_point, size), None);
        assert!(scene.set_pointer(place_point, size));
        assert_eq!(scene.hovered_place, Some(0));
        assert_eq!(scene.hovered_item, None);

        let item = scene.layout(size).item(0).expect("item should layout");
        let item_point = ViewPoint {
            x: scene.content_origin_x(size) + item.visual_rect.x + 2.0,
            y: scene.content_origin_y() + item.visual_rect.y + 2.0,
        };
        assert!(scene.set_pointer(item_point, size));
        assert_eq!(scene.hovered_place, None);
        assert_eq!(
            scene.hovered_item,
            Some(ShellPaneItemTarget {
                pane: ShellPaneId::SLOT_0,
                index: 0,
            })
        );
    }
