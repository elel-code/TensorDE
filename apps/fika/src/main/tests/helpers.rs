    use crate::ui::render::dmabuf::GPU_TEST_LOCK;

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
        std::env::temp_dir().join(format!("fika-{name}-{unique}"))
    }

    fn wait_for_thumbnail_state(
        resolver: &mut ThumbnailSourceResolver,
        path: &Path,
        modified_secs: u64,
        mime_type: Option<&str>,
        size_px: u16,
    ) -> ThumbnailResolveState {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let state =
                resolver.resolve(path, modified_secs, mime_type.map(str::to_string), size_px);
            if !matches!(state, ThumbnailResolveState::Pending) || Instant::now() >= deadline {
                return state;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn write_test_thumbnail_png(path: &Path, uri: &str, modified_secs: u64) {
        write_test_thumbnail_png_with_color(path, uri, modified_secs, [32, 96, 192, 255]);
    }

    fn write_test_thumbnail_png_with_color(
        path: &Path,
        uri: &str,
        modified_secs: u64,
        color: [u8; 4],
    ) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        image::RgbaImage::from_pixel(4, 4, image::Rgba(color))
            .save(path)
            .unwrap();
        fika_core::write_thumbnail_metadata(path, uri, modified_secs).unwrap();
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
        assert!(ui::item_reflow::has_pending_item_reflow(&scene));
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

        assert!(ui::item_reflow::start_due_item_reflow_transitions(
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
        assert!(!ui::item_reflow::has_pending_item_reflow(&scene));
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

    #[test]
    fn settled_visible_page_resolves_mime_without_a_scroll_event() {
        let root = test_dir("visible-mime-without-scroll");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("extensionless-image"),
            b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR",
        )
        .unwrap();
        let mut scene = ShellScene::load(root.clone(), ShellViewMode::Compact).unwrap();
        let size = PhysicalSize::new(1100, 720);
        assert!(!scene.panes[ShellPaneId::SLOT_0].entries[0].mime_magic_checked);

        let paused = synchronize_visible_metadata_roles(&mut scene, size, true);
        assert_eq!(paused.applied, 0);
        assert!(!scene.panes[ShellPaneId::SLOT_0].entries[0].mime_magic_checked);

        let settled = synchronize_visible_metadata_roles(&mut scene, size, false);
        assert_eq!(settled.visible, 1);
        assert_eq!(settled.applied, 1);
        let entry = &scene.panes[ShellPaneId::SLOT_0].entries[0];
        assert!(entry.mime_magic_checked);
        assert_eq!(entry.mime_type.as_deref(), Some("image/png"));

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn metadata_read_ahead_pumps_all_batches_without_another_prewarm() {
        const ITEM_COUNT: usize = METADATA_ROLE_BATCH_SIZE * 2 + 5;
        let root = test_dir("metadata-read-ahead-pump");
        std::fs::create_dir_all(&root).unwrap();
        for index in 0..ITEM_COUNT {
            std::fs::write(
                root.join(format!("extensionless-{index:03}")),
                b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR",
            )
            .unwrap();
        }
        let mut scene = ShellScene::load(root.clone(), ShellViewMode::Compact).unwrap();
        let size = PhysicalSize::new(640, 360);
        let layouts = scene.prepare_frame_projection_layouts(size);
        let projections = scene.pane_projections_from_layouts(layouts);
        let initial = scene.prewarm_file_metadata_roles(projections.projections());
        drop(projections);

        assert_eq!(initial.batches_started, 1);
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut applied = 0;
        let mut batches_started = initial.batches_started;
        while applied < ITEM_COUNT && Instant::now() < deadline {
            let stats = scene.drain_metadata_role_results();
            applied += stats.applied;
            batches_started += stats.batches_started;
            if stats.results == 0 {
                std::thread::yield_now();
            }
        }

        assert_eq!(applied, ITEM_COUNT);
        assert_eq!(batches_started, ITEM_COUNT.div_ceil(METADATA_ROLE_BATCH_SIZE));
        assert!(scene.panes[ShellPaneId::SLOT_0]
            .entries
            .iter()
            .all(|entry| entry.mime_magic_checked && entry.mime_type.as_deref() == Some("image/png")));

        std::fs::remove_dir_all(root).unwrap();
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

    /// Fluent fixture builder for in-memory [`ShellScene`] tests.
    ///
    /// Defaults come from [`ShellScene::from_primary_pane`]. Prefer this over
    /// mutating scene fields after `test_scene` when adding common presets.
    #[derive(Clone, Debug)]
    struct TestShellSceneBuilder {
        path: PathBuf,
        entries: Vec<Entry>,
        view_mode: ShellViewMode,
        show_hidden: bool,
        dark_mode: bool,
        places_visible: bool,
        scale_factor: f32,
        places: Option<Vec<ShellPlace>>,
        trash_has_items: bool,
        secondary: Option<TestShellScenePane>,
    }

    #[derive(Clone, Debug)]
    struct TestShellScenePane {
        path: PathBuf,
        view_mode: ShellViewMode,
        entries: Vec<Entry>,
        zoom_level: Option<i32>,
    }

    impl TestShellSceneBuilder {
        fn new() -> Self {
            Self {
                path: PathBuf::from("/tmp"),
                entries: Vec::new(),
                view_mode: ShellViewMode::Icons,
                show_hidden: false,
                dark_mode: false,
                places_visible: true,
                scale_factor: 1.0,
                places: None,
                trash_has_items: false,
                secondary: None,
            }
        }

        fn with_path(mut self, path: impl Into<PathBuf>) -> Self {
            self.path = path.into();
            self
        }

        fn with_entries(mut self, entries: Vec<Entry>) -> Self {
            self.entries = entries;
            self
        }

        fn with_view_mode(mut self, view_mode: ShellViewMode) -> Self {
            self.view_mode = view_mode;
            self
        }

        fn with_show_hidden(mut self, show_hidden: bool) -> Self {
            self.show_hidden = show_hidden;
            self
        }

        fn with_dark_mode(mut self, dark_mode: bool) -> Self {
            self.dark_mode = dark_mode;
            self
        }

        fn with_places_visible(mut self, places_visible: bool) -> Self {
            self.places_visible = places_visible;
            self
        }

        fn with_scale_factor(mut self, scale_factor: f32) -> Self {
            self.scale_factor = scale_factor;
            self
        }

        fn with_trash_has_items(mut self, trash_has_items: bool) -> Self {
            self.trash_has_items = trash_has_items;
            self
        }

        /// Open the secondary split pane with the given path/entries.
        fn with_secondary_pane(
            mut self,
            path: impl Into<PathBuf>,
            view_mode: ShellViewMode,
            entries: Vec<Entry>,
        ) -> Self {
            self.secondary = Some(TestShellScenePane {
                path: path.into(),
                view_mode,
                entries,
                zoom_level: None,
            });
            self
        }

        fn with_secondary_zoom_level(mut self, zoom_level: i32) -> Self {
            if let Some(secondary) = self.secondary.as_mut() {
                secondary.zoom_level = Some(zoom_level);
            }
            self
        }

        fn build(self) -> ShellScene {
            let places = self.places.unwrap_or_else(|| {
                vec![
                    ShellPlace::new("", "H", "Home", self.path.clone(), false),
                    ShellPlace::new("Devices", "/", "Root", PathBuf::from("/"), false),
                ]
            });
            let mut scene = ShellScene::from_primary_pane(
                ShellPaneState::from_entries(
                    self.path,
                    self.view_mode,
                    self.entries,
                    self.show_hidden,
                    "",
                ),
                places,
                self.trash_has_items,
                self.show_hidden,
            );
            scene.dark_mode = self.dark_mode;
            scene.places_visible = self.places_visible;
            scene.scale_factor = self.scale_factor;
            if let Some(secondary) = self.secondary {
                set_test_pane_with_zoom_level(
                    &mut scene,
                    ShellPaneId::SLOT_1,
                    secondary.path,
                    secondary.view_mode,
                    secondary.entries,
                    secondary.zoom_level,
                );
            }
            scene
        }
    }

    /// In-memory scene fixture with deterministic places (no disk listing).
    fn test_scene(entries: Vec<Entry>, view_mode: ShellViewMode) -> ShellScene {
        TestShellSceneBuilder::new()
            .with_entries(entries)
            .with_view_mode(view_mode)
            .build()
    }

    fn set_test_pane(
        scene: &mut ShellScene,
        pane: ShellPaneId,
        path: PathBuf,
        view_mode: ShellViewMode,
        entries: Vec<Entry>,
    ) {
        set_test_pane_with_zoom_level(scene, pane, path, view_mode, entries, None);
    }

    fn set_test_pane_with_zoom_level(
        scene: &mut ShellScene,
        pane: ShellPaneId,
        path: PathBuf,
        view_mode: ShellViewMode,
        entries: Vec<Entry>,
        zoom_level: Option<i32>,
    ) {
        let dir_count = entries.iter().filter(|entry| entry.is_dir).count();
        let filtered_indexes = filtered_indexes_for_entries(
            &entries,
            scene.show_hidden,
            scene.filter_pattern_for_pane(pane),
        );
        let mut state = ShellPaneState {
            path,
            pending_path: None,
            view_mode,
            zoom_levels: ShellPaneZoomLevels::default(),
            dir_count,
            filtered_indexes,
            entries,
            selection: ShellSelection::default(),
            scroll_x: 0.0,
            scroll_y: 0.0,
        };
        if let Some(zoom_level) = zoom_level {
            state.set_zoom_level(zoom_level);
        }
        scene.panes.set(pane, state);
    }

    fn filtered_names(scene: &ShellScene, pane: ShellPaneId) -> Vec<String> {
        let pane = scene.pane_state(pane).expect("test pane should be open");
        pane.filtered_indexes
            .iter()
            .map(|index| pane.entries[*index].name.as_ref().to_string())
            .collect()
    }

    #[test]
    fn test_shell_scene_builder_applies_presets() {
        let scene = TestShellSceneBuilder::new()
            .with_path("/fixture")
            .with_entries(vec![test_entry("a.txt", false)])
            .with_view_mode(ShellViewMode::Details)
            .with_show_hidden(true)
            .with_dark_mode(true)
            .with_places_visible(false)
            .with_scale_factor(1.5)
            .with_trash_has_items(true)
            .with_secondary_pane(
                "/right",
                ShellViewMode::Compact,
                vec![test_entry("b.txt", false)],
            )
            .with_secondary_zoom_level(2)
            .build();
        assert_eq!(
            scene.panes[ShellPaneId::SLOT_0].path,
            PathBuf::from("/fixture")
        );
        assert_eq!(
            scene.panes[ShellPaneId::SLOT_0].view_mode,
            ShellViewMode::Details
        );
        assert!(scene.show_hidden);
        assert!(scene.dark_mode);
        assert!(!scene.places_visible);
        assert_eq!(scene.scale_factor, 1.5);
        assert!(scene.trash_has_items);
        assert_eq!(
            scene.panes[ShellPaneId::SLOT_1].path,
            PathBuf::from("/right")
        );
        assert_eq!(
            scene.panes[ShellPaneId::SLOT_1].view_mode,
            ShellViewMode::Compact
        );
        assert_eq!(scene.panes[ShellPaneId::SLOT_1].zoom_level(), 2);
        assert_eq!(scene.panes[ShellPaneId::SLOT_1].entries.len(), 1);
    }

    #[test]
    fn places_hit_testing_is_separate_from_file_content() {
        let mut scene = TestShellSceneBuilder::new()
            .with_entries(vec![test_entry("alpha.txt", false)])
            .build();
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
