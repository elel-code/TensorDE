
    #[test]
    fn location_draft_replaces_completes_and_cancels() {
        let temp = test_dir("location-draft");
        std::fs::create_dir_all(temp.join("alpha")).unwrap();

        let mut scene = test_scene(vec![test_entry("alpha", true)], ShellViewMode::Icons);
        scene.panes[ShellPaneId::SLOT_0].path = temp.clone();
        let size = PhysicalSize::new(420, 260);

        assert!(scene.apply_location_command(LocationCommand::Activate, size));
        let initial_value = temp.display().to_string();
        assert_eq!(scene.location_draft_value(), Some(initial_value.as_str()));
        assert!(scene.next_animation_frame_deadline().is_some());
        assert_ne!(
            scene.animation_dirty_value_with_hover(true),
            scene.animation_dirty_value_with_hover(false)
        );

        assert!(scene.apply_location_command(LocationCommand::Insert("a".to_string()), size));
        assert_eq!(scene.location_draft_value(), Some("a"));

        assert!(scene.apply_location_command(LocationCommand::Complete, size));
        assert_eq!(scene.location_draft_value(), Some("alpha/"));
        assert_eq!(
            scene.resolved_location_draft(),
            Some((ShellPaneId::SLOT_0, temp.join("alpha/")))
        );

        assert!(scene.apply_location_command(LocationCommand::Cancel, size));
        assert_eq!(scene.location_draft_value(), None);
        assert!(!scene.is_location_editing());
        assert!(!scene.animation_active());

        let _ = std::fs::remove_dir_all(temp);
    }

    #[test]
    fn location_draft_cursor_edits_at_caret() {
        let mut scene = test_scene(vec![test_entry("alpha", false)], ShellViewMode::Icons);
        scene.panes[ShellPaneId::SLOT_0].path = PathBuf::from("/tmp");
        let size = PhysicalSize::new(420, 260);

        assert!(scene.apply_location_command(LocationCommand::Activate, size));
        let draft = scene.location_draft.as_ref().unwrap();
        assert_eq!(draft.pane, ShellPaneId::SLOT_0);
        assert_eq!(draft.draft.cursor, draft.draft.value.len());

        assert!(scene.apply_location_command(LocationCommand::Insert("abc".to_string()), size));
        assert_eq!(scene.location_draft_value(), Some("abc"));
        assert_eq!(scene.location_draft.as_ref().unwrap().draft.cursor, 3);

        assert!(scene.apply_location_command(LocationCommand::MoveLeft, size));
        assert_eq!(scene.location_draft.as_ref().unwrap().draft.cursor, 2);
        assert!(scene.apply_location_command(LocationCommand::Backspace, size));
        assert_eq!(scene.location_draft_value(), Some("ac"));
        assert_eq!(scene.location_draft.as_ref().unwrap().draft.cursor, 1);

        assert!(scene.apply_location_command(LocationCommand::Insert("β".to_string()), size));
        assert_eq!(scene.location_draft_value(), Some("aβc"));
        assert_eq!(
            scene.location_draft.as_ref().unwrap().draft.cursor,
            "aβ".len()
        );

        assert!(scene.apply_location_command(LocationCommand::MoveLeft, size));
        assert_eq!(
            scene.location_draft.as_ref().unwrap().draft.cursor,
            "a".len()
        );
        assert!(scene.apply_location_command(LocationCommand::Delete, size));
        assert_eq!(scene.location_draft_value(), Some("ac"));
        assert_eq!(scene.location_draft.as_ref().unwrap().draft.cursor, 1);

        assert!(scene.apply_location_command(LocationCommand::MoveEnd, size));
        assert_eq!(scene.location_draft.as_ref().unwrap().draft.cursor, 2);
        assert!(scene.apply_location_command(LocationCommand::MoveHome, size));
        assert_eq!(scene.location_draft.as_ref().unwrap().draft.cursor, 0);
    }

    #[test]
    fn location_bar_mouse_click_places_caret_without_resetting_draft() {
        let mut scene = test_scene(vec![test_entry("alpha", false)], ShellViewMode::Icons);
        scene.panes[ShellPaneId::SLOT_0].path = PathBuf::from("/tmp/alpha");
        let size = PhysicalSize::new(640, 360);
        let path_bar = scene.path_bar_rect(size).unwrap();
        let text_rect = scene.location_text_rect_for_path_bar_rect(path_bar);
        let label = scene.location_label_for_pane(ShellPaneId::SLOT_0);
        let tmp_cursor = "/tmp".len();
        let tmp_x = scene.text_hit_tests.borrow_mut().cursor_x(
            &label,
            tmp_cursor,
            TextCursorLayout {
                rect: text_rect,
                alignment: LabelAlignment::Start,
                wrap: LabelWrap::None,
                max_font_size: scene.scale_metric(TEXT_FONT_SIZE),
                max_line_height: scene.text_line_height(),
            },
        );

        assert!(scene.activate_path_bar_at_screen_point(
            ViewPoint {
                x: text_rect.x + tmp_x,
                y: text_rect.y + text_rect.height / 2.0,
            },
            size
        ));
        let draft = scene.location_draft.as_ref().unwrap();
        assert_eq!(draft.draft.cursor, tmp_cursor);
        assert!(!draft.draft.replace_on_insert);

        assert!(scene.apply_location_command(LocationCommand::Insert("X".to_string()), size));
        assert_eq!(scene.location_draft_value(), Some("/tmpX/alpha"));

        let edited = scene.location_draft_value().unwrap().to_string();
        let tail_x = scene.text_hit_tests.borrow_mut().cursor_x(
            &edited,
            edited.len(),
            TextCursorLayout {
                rect: text_rect,
                alignment: LabelAlignment::Start,
                wrap: LabelWrap::None,
                max_font_size: scene.scale_metric(TEXT_FONT_SIZE),
                max_line_height: scene.text_line_height(),
            },
        );
        assert!(scene.activate_path_bar_at_screen_point(
            ViewPoint {
                x: text_rect.x + tail_x + 20.0,
                y: text_rect.y + text_rect.height / 2.0,
            },
            size
        ));
        let draft = scene.location_draft.as_ref().unwrap();
        assert_eq!(draft.draft.value, "/tmpX/alpha");
        assert_eq!(draft.draft.cursor, "/tmpX/alpha".len());
    }

    #[test]
    fn text_caret_blink_tracks_location_and_open_with_inputs() {
        let mut scene = test_scene(vec![test_entry("note.txt", false)], ShellViewMode::Icons);
        scene.panes[ShellPaneId::SLOT_0].path = PathBuf::from("/tmp");
        let size = PhysicalSize::new(640, 460);

        assert!(!scene.location_text_caret_active());
        assert!(!scene.open_with_text_caret_active());
        assert!(scene.next_text_caret_blink_deadline().is_none());
        assert_eq!(scene.location_text_caret_dirty_value(), 0);

        assert!(scene.apply_location_command(LocationCommand::Activate, size));
        assert!(scene.location_text_caret_active());
        assert!(scene.text_caret_blink_active());
        assert!(scene.text_caret_visible());
        assert!(scene.next_text_caret_blink_deadline().is_some());
        assert_ne!(scene.location_text_caret_dirty_value(), 0);
        let editing_key = ShellRenderDirtyKey::from_scene(&scene, size);
        let editing_hoverless = ShellRenderDirtyKey::from_scene_ignoring_hover(&scene, size);
        assert_ne!(editing_key, editing_hoverless);

        assert!(scene.close_location_draft(size));
        assert!(!scene.location_text_caret_active());
        assert_eq!(scene.location_text_caret_dirty_value(), 0);

        scene.open_with_chooser = Some(ShellOpenWithChooser::new(
            PathBuf::from("/tmp/note.txt"),
            Some(Arc::from("text/plain")),
            vec![MimeApplication {
                id: "paint.desktop".to_string(),
                desktop_file: PathBuf::from("/apps/paint.desktop"),
                name: "Paint".to_string(),
                exec: "paint %f".to_string(),
                icon: None,
                is_default: false,
            }],
            Vec::new(),
        ));
        assert!(scene.open_with_text_caret_active());
        assert!(scene.text_caret_blink_active());
        assert!(scene.next_text_caret_blink_deadline().is_some());
        assert_ne!(scene.open_with_text_caret_dirty_value(), 0);
    }

    #[test]
    fn location_draft_blurs_outside_path_bar_without_committing() {
        let mut scene = test_scene(vec![test_entry("alpha", false)], ShellViewMode::Icons);
        scene.panes[ShellPaneId::SLOT_0].path = PathBuf::from("/tmp");
        let size = PhysicalSize::new(600, 320);

        assert!(scene.apply_location_command(LocationCommand::Activate, size));
        assert!(
            scene.apply_location_command(
                LocationCommand::Insert("/does-not-exist".to_string()),
                size
            )
        );
        assert_eq!(scene.location_draft_value(), Some("/does-not-exist"));

        let path_bar = scene.path_bar_rect(size).unwrap();
        assert!(!scene.close_location_draft_if_outside(
            ViewPoint {
                x: path_bar.x + 4.0,
                y: path_bar.y + 4.0,
            },
            size
        ));
        assert!(scene.is_location_editing());

        let blank = ViewPoint {
            x: scene.content_origin_x(size) + scene.content_width(size) - 4.0,
            y: scene.content_origin_y() + 4.0,
        };
        assert!(scene.close_location_draft_if_outside(blank, size));
        assert_eq!(scene.location_draft_value(), None);
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].path, PathBuf::from("/tmp"));
        assert_eq!(scene.location_changes, 3);
    }

    #[test]
    fn text_hit_test_runtime_uses_shaped_glyph_boundaries() {
        let mut runtime = TextHitTestRuntime::new();
        let rect = ViewRect {
            x: 0.0,
            y: 0.0,
            width: 220.0,
            height: TEXT_LINE_HEIGHT,
        };
        let label = "mmmmii";
        let first = runtime.cursor_x(
            label,
            1,
            TextCursorLayout {
                rect,
                alignment: LabelAlignment::Start,
                wrap: LabelWrap::None,
                max_font_size: TEXT_FONT_SIZE,
                max_line_height: TEXT_LINE_HEIGHT,
            },
        );
        let second = runtime.cursor_x(
            label,
            2,
            TextCursorLayout {
                rect,
                alignment: LabelAlignment::Start,
                wrap: LabelWrap::None,
                max_font_size: TEXT_FONT_SIZE,
                max_line_height: TEXT_LINE_HEIGHT,
            },
        );

        assert!(second > first);
        assert_eq!(
            runtime.cursor_for_offset(
                label,
                rect,
                first + (second - first) * 0.25,
                LabelAlignment::Start,
                LabelWrap::None,
                1.0,
            ),
            1
        );
        assert_eq!(
            runtime.cursor_for_offset(
                label,
                rect,
                first + (second - first) * 0.75,
                LabelAlignment::Start,
                LabelWrap::None,
                1.0,
            ),
            2
        );
    }

    #[test]
    fn shaped_label_cursor_measurement_tracks_glyph_layout() {
        let mut font_system = FontSystem::new();
        let mut swash_cache = SwashCache::new();
        let mut text_buffer = Buffer::new_empty(Metrics::new(TEXT_FONT_SIZE, TEXT_LINE_HEIGHT));
        let mut label_cache = LabelRasterCache::new(1024 * 1024);
        let mut metrics_cache = LabelMetricsCache::new(TEXT_LABEL_METRICS_CACHE_MAX_ENTRIES);
        let mut atlas_cache = TextAtlasFrameCache::default();
        let mut text = TextFrameBuilder::new(
            TextFrameResources::new(
                &mut font_system,
                &mut swash_cache,
                &mut text_buffer,
                &mut label_cache,
                &mut metrics_cache,
                &mut atlas_cache,
            ),
            PhysicalSize::new(320, 120),
            1.0,
            Vec::new(),
        );
        let rect = ViewRect {
            x: 0.0,
            y: 0.0,
            width: 220.0,
            height: TEXT_LINE_HEIGHT,
        };

        let one =
            text.measure_label_cursor_x("abcdef", rect, 1, LabelAlignment::Start, LabelWrap::None);
        let end = text.measure_label_cursor_x(
            "abcdef",
            rect,
            "abcdef".len(),
            LabelAlignment::Start,
            LabelWrap::None,
        );
        let wide = text.measure_label_cursor_x(
            "mmmmmm",
            rect,
            "mmmmmm".len(),
            LabelAlignment::Start,
            LabelWrap::None,
        );

        assert!(one > 0.0);
        assert!(end > one);
        assert!(wide > end);
    }

    #[test]
    fn text_raster_misses_are_budgeted_across_frames() {
        let mut font_system = FontSystem::new();
        let mut swash_cache = SwashCache::new();
        let mut text_buffer = Buffer::new_empty(Metrics::new(TEXT_FONT_SIZE, TEXT_LINE_HEIGHT));
        let mut label_cache = LabelRasterCache::new(1024 * 1024);
        let mut metrics_cache = LabelMetricsCache::new(TEXT_LABEL_METRICS_CACHE_MAX_ENTRIES);
        let mut atlas_cache = TextAtlasFrameCache::default();
        let rect = ViewRect {
            x: 0.0,
            y: 0.0,
            width: 180.0,
            height: TEXT_LINE_HEIGHT,
        };
        let clip = ViewRect {
            x: 0.0,
            y: 0.0,
            width: 240.0,
            height: 120.0,
        };

        let mut first = TextFrameBuilder::new(
            TextFrameResources::new(
                &mut font_system,
                &mut swash_cache,
                &mut text_buffer,
                &mut label_cache,
                &mut metrics_cache,
                &mut atlas_cache,
            ),
            PhysicalSize::new(320, 120),
            1.0,
            Vec::new(),
        );
        for index in 0..=TEXT_RASTER_MISS_BUDGET_PER_FRAME {
            first.push_label(
                &format!("label-{index}"),
                rect,
                clip,
                TextColor::rgb(36, 41, 47),
            );
        }
        let first_frame = first.finish();
        assert_eq!(first_frame.stats.quads, TEXT_RASTER_MISS_BUDGET_PER_FRAME);
        assert_eq!(first_frame.stats.deferred, 1);

        let mut second = TextFrameBuilder::new(
            TextFrameResources::new(
                &mut font_system,
                &mut swash_cache,
                &mut text_buffer,
                &mut label_cache,
                &mut metrics_cache,
                &mut atlas_cache,
            ),
            PhysicalSize::new(320, 120),
            1.0,
            Vec::new(),
        );
        for index in 0..=TEXT_RASTER_MISS_BUDGET_PER_FRAME {
            second.push_label(
                &format!("label-{index}"),
                rect,
                clip,
                TextColor::rgb(36, 41, 47),
            );
        }
        let second_frame = second.finish();
        assert_eq!(
            second_frame.stats.quads,
            TEXT_RASTER_MISS_BUDGET_PER_FRAME + 1
        );
        assert_eq!(second_frame.stats.deferred, 0);
    }

    #[test]
    fn text_atlas_reuploads_reused_slot_after_label_cache_eviction() {
        let mut font_system = FontSystem::new();
        let mut swash_cache = SwashCache::new();
        let mut text_buffer = Buffer::new_empty(Metrics::new(TEXT_FONT_SIZE, TEXT_LINE_HEIGHT));
        let mut label_cache = LabelRasterCache::new(1024 * 1024);
        let mut metrics_cache = LabelMetricsCache::new(TEXT_LABEL_METRICS_CACHE_MAX_ENTRIES);
        let mut atlas_cache = TextAtlasFrameCache::default();
        let rect = ViewRect {
            x: 0.0,
            y: 0.0,
            width: 180.0,
            height: TEXT_LINE_HEIGHT,
        };
        let clip = ViewRect {
            x: 0.0,
            y: 0.0,
            width: 240.0,
            height: 120.0,
        };

        let mut first = TextFrameBuilder::new(
            TextFrameResources::new(
                &mut font_system,
                &mut swash_cache,
                &mut text_buffer,
                &mut label_cache,
                &mut metrics_cache,
                &mut atlas_cache,
            ),
            PhysicalSize::new(320, 120),
            1.0,
            Vec::new(),
        );
        first.push_label("alpha", rect, clip, TextColor::rgb(36, 41, 47));
        let first_frame = first.finish();
        assert_eq!(first_frame.stats.atlas_reused, 0);
        assert_eq!(first_frame.uploads.len(), 1);

        label_cache.entries.clear();
        label_cache.bytes = 0;

        let mut second = TextFrameBuilder::new(
            TextFrameResources::new(
                &mut font_system,
                &mut swash_cache,
                &mut text_buffer,
                &mut label_cache,
                &mut metrics_cache,
                &mut atlas_cache,
            ),
            PhysicalSize::new(320, 120),
            1.0,
            Vec::new(),
        );
        second.push_label("alpha", rect, clip, TextColor::rgb(36, 41, 47));
        let second_frame = second.finish();

        assert_eq!(second_frame.stats.atlas_reused, 1);
        assert_eq!(second_frame.uploads.len(), 1);
    }

    #[test]
    fn text_label_cache_keeps_recent_recycled_entries_only() {
        let mut cache = LabelRasterCache::new(1024 * 1024);
        for index in 0..150 {
            cache.begin_frame();
            cache.insert(
                LabelCacheKey {
                    text: format!("label-{index}"),
                    width: 8,
                    height: 8,
                    alignment: LabelAlignment::Start,
                    wrap: LabelWrap::None,
                },
                vec![index as u8; 64],
            );
        }

        assert!(cache.evict_to_recent_entry_limit(TEXT_LABEL_RECYCLE_CACHE_ENTRIES));
        assert_eq!(cache.len(), TEXT_LABEL_RECYCLE_CACHE_ENTRIES);
        assert!(!cache.contains_key(&LabelCacheKey {
            text: "label-0".to_string(),
            width: 8,
            height: 8,
            alignment: LabelAlignment::Start,
            wrap: LabelWrap::None,
        }));
        assert!(cache.contains_key(&LabelCacheKey {
            text: "label-149".to_string(),
            width: 8,
            height: 8,
            alignment: LabelAlignment::Start,
            wrap: LabelWrap::None,
        }));
    }
