
    #[test]
    fn split_pane_status_zoom_targets_the_hit_pane_only() {
        let mut scene = TestShellSceneBuilder::new()
            .with_entries(vec![test_entry("left.txt", false)])
            .with_secondary_pane(
                "/right-root",
                ShellViewMode::Icons,
                vec![test_entry("right.txt", false)],
            )
            .with_places_visible(false)
            .build();
        let size = PhysicalSize::new(1400, 520);
        let right_zoom = scene
            .status_zoom_indicator_rects_for_pane(ShellPaneId::SLOT_1, size)
            .expect("wide split pane should show right zoom indicator");
        let high = ViewPoint {
            x: right_zoom.track.right() - 1.0,
            y: right_zoom.track.y + right_zoom.track.height / 2.0,
        };

        assert_eq!(scene.begin_scrollbar_drag(high, size), Some(true));
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].zoom_step, 0);
        assert_eq!(scene.panes[ShellPaneId::SLOT_1].zoom_step, ZOOM_STEP_MAX);
        assert_eq!(
            scene.scrollbar_drag.map(|drag| drag.target),
            Some(ScrollbarDragTarget::StatusZoom {
                pane: ShellPaneId::SLOT_1
            })
        );
    }

    #[test]
    fn icons_layout_allocates_multiple_text_lines_for_long_names() {
        let long_name = "a-very-long-folder-name-that-needs-more-than-one-line-in-icons-layout.png";
        let scene = test_scene(vec![test_entry(long_name, false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(420, 260);
        let options = scene.icons_options(size);
        let item = match scene.layout(size) {
            ShellLayout::Icons(layout) => layout.item(0).unwrap(),
            _ => unreachable!(),
        };

        assert!(item.text_rect.height >= scene.text_line_height() * 2.0);
        assert!(
            item.text_rect.height <= scene.text_line_height() * DOLPHIN_ICONS_MAX_TEXT_LINES as f32
        );
        assert!(item.item_rect.height > options.item_height);
        assert!(item.text_rect.bottom() <= item.item_rect.bottom() + f32::EPSILON);
    }

    #[test]
    fn icons_layout_height_cache_reuses_name_measurements_while_scrolling() {
        let mut scene = test_scene(
            (0..80)
                .map(|index| {
                    test_entry(
                        &format!("very-long-file-name-that-needs-icons-wrapping-{index:02}.txt"),
                        false,
                    )
                })
                .collect(),
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(420, 260);

        assert_eq!(scene.icons_layout_height_cache.len(), 0);
        let first_visible_before = match scene.layout(size) {
            ShellLayout::Icons(layout) => layout.visible_items().next().unwrap().model_index,
            _ => unreachable!(),
        };
        assert_eq!(scene.icons_layout_height_cache.len(), 1);

        scene.panes[ShellPaneId::SLOT_0].scroll_y = 180.0;
        let first_visible_after = match scene.layout(size) {
            ShellLayout::Icons(layout) => layout.visible_items().next().unwrap().model_index,
            _ => unreachable!(),
        };

        assert_eq!(scene.icons_layout_height_cache.len(), 1);
        assert!(first_visible_after > first_visible_before);
    }

    #[test]
    fn dolphin_filename_elision_preserves_extension() {
        let mut font_system = FontSystem::new();
        let mut buffer = Buffer::new_empty(Metrics::new(TEXT_FONT_SIZE, TEXT_LINE_HEIGHT));
        let display = dolphin_elide_filename_to_width_shaped(
            &mut font_system,
            &mut buffer,
            "very-long-filename-that-does-not-fit-in-details-mode.tar.gz",
            130.0,
            TEXT_FONT_SIZE,
            TEXT_LINE_HEIGHT,
        );

        assert!(display.contains('…'));
        assert!(!display.contains("..."));
        assert!(display.ends_with(".gz"));
        assert!(
            dolphin_text_width_no_wrap(
                &mut font_system,
                &mut buffer,
                &display,
                TEXT_FONT_SIZE,
                TEXT_LINE_HEIGHT,
            ) <= 130.0
        );
    }

    #[test]
    fn dolphin_wrapped_filename_elides_only_last_icons_line() {
        let mut font_system = FontSystem::new();
        let mut buffer = Buffer::new_empty(Metrics::new(TEXT_FONT_SIZE, TEXT_LINE_HEIGHT));
        let layout = dolphin_layout_icons_filename(
            &mut font_system,
            &mut buffer,
            "very-long-folder-preview-name-that-needs-more-than-three-lines.png",
            (64.0 - TEXT_PADDING as f32 * 2.0).max(1.0),
            DOLPHIN_ICONS_MAX_TEXT_LINES,
            TEXT_FONT_SIZE,
            TEXT_LINE_HEIGHT,
        );
        let visible_display = layout.display.replace('\u{200B}', "");

        assert!(layout.elided);
        assert_eq!(layout.line_count, DOLPHIN_ICONS_MAX_TEXT_LINES);
        assert!(visible_display.contains('…'));
        assert!(!visible_display.contains("..."));
        assert!(visible_display.ends_with(".png"));
    }

    #[test]
    fn transition_scaled_icons_text_reuses_unscaled_dolphin_layout_key() {
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
            PhysicalSize::new(320, 180),
            1.0,
            Vec::new(),
        );
        let label = "very-long-folder-preview-name-that-needs-more-than-three-lines.png";
        let layout_rect = ViewRect {
            x: 20.0,
            y: 20.0,
            width: 92.0,
            height: TEXT_LINE_HEIGHT * DOLPHIN_ICONS_MAX_TEXT_LINES as f32,
        };
        let scaled_rect = ViewRect {
            x: 43.0,
            y: 29.0,
            width: 46.0,
            height: layout_rect.height * 0.5,
        };
        let clip = ViewRect {
            x: 0.0,
            y: 0.0,
            width: 320.0,
            height: 180.0,
        };

        text.push_filename_label_wrapped_with_layout(
            label,
            layout_rect,
            layout_rect,
            clip,
            TextColor::rgb(36, 41, 47),
        );
        text.push_filename_label_wrapped_with_layout(
            label,
            scaled_rect,
            layout_rect,
            clip,
            TextColor::rgb(36, 41, 47),
        );
        let frame = text.finish();

        assert_eq!(frame.stats.cache_misses, 1);
        assert_eq!(frame.stats.cache_hits, 1);
        assert_eq!(frame.stats.quads, 2);
    }

    #[test]
    fn compact_text_width_uses_estimated_glyph_widths_not_name_length() {
        let narrow = compact_entry_text_width(&test_entry("iiiiiiiiiiiiiiii.txt", false), 1.0);
        let wide = compact_entry_text_width(&test_entry("mmmmmmmm.txt", false), 1.0);

        assert!(wide > narrow);
    }

    #[test]
    fn compact_layout_and_prewarm_keep_full_item_name() {
        let long_name =
            "very-long-compact-name-that-dolphin-keeps-unelided-by-expanding-the-column.tar.gz";
        let scene = test_scene(vec![test_entry(long_name, false)], ShellViewMode::Compact);
        let size = PhysicalSize::new(320, 180);
        let options = scene.compact_options(size);
        let layout = match scene.layout(size) {
            ShellLayout::Compact(layout) => layout,
            _ => unreachable!(),
        };
        let item = layout.item(0).unwrap();
        let text_width = compact_entry_text_width(&test_entry(long_name, false), scene.ui_scale());

        assert!(item.text_rect.width >= text_width - 1.0);
        assert!(
            item.item_rect.width >= required_compact_item_width(options, text_width) - f32::EPSILON
        );

        let projection = scene
            .pane_projection(ShellPaneId::SLOT_0, size)
            .expect("compact projection");
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
            size,
            scene.ui_scale(),
            Vec::new(),
        );

        let outcome =
            scene.prewarm_projection_text_label(&projection, item, &mut text, scene.theme());
        drop(text);

        assert!(matches!(
            outcome,
            LabelCacheOutcome::Miss | LabelCacheOutcome::Hit
        ));
        assert!(label_cache.entries.keys().any(|key| key.text == long_name));
        assert!(
            !label_cache
                .entries
                .keys()
                .any(|key| key.text.contains('…') || key.text.contains("..."))
        );
    }

    #[test]
    fn window_scale_factor_scales_default_shell_metrics() {
        let mut scene = TestShellSceneBuilder::new()
            .with_entries(vec![test_entry("alpha.txt", false)])
            .with_secondary_pane(
                "/right-root",
                ShellViewMode::Icons,
                (0..80)
                    .map(|index| test_entry(&format!("right-{index:02}.txt"), false))
                    .collect(),
            )
            .build();
        scene.panes[ShellPaneId::SLOT_1].scroll_y = 80.0;
        let size = PhysicalSize::new(900, 600);

        assert!(scene.set_scale_factor(1.5, size));
        assert_eq!(scene.top_bar_height(), 54.0);
        assert_eq!(scene.text_line_height(), 27.0);
        assert_eq!(scene.panes[ShellPaneId::SLOT_1].scroll_y, 120.0);

        let icons_item = match scene.layout(size) {
            ShellLayout::Icons(layout) => layout.item(0).unwrap(),
            _ => unreachable!(),
        };
        assert_eq!(scene.icons_options(size).text_height, 27.0);
        assert_eq!(icons_item.icon_rect.width, 72.0);
        assert!(icons_item.text_rect.height >= 27.0);

        assert!(scene.set_view_mode(ShellViewMode::Compact, size));
        let compact_item = match scene.layout(size) {
            ShellLayout::Compact(layout) => layout.item(0).unwrap(),
            _ => unreachable!(),
        };
        assert_eq!(scene.compact_options(size).text_height, 27.0);
        assert_eq!(compact_item.icon_rect.width, 42.0);
        assert!(compact_item.text_rect.height >= 27.0);

        assert!(scene.set_view_mode(ShellViewMode::Details, size));
        let details_item = match scene.layout(size) {
            ShellLayout::Details(layout) => layout.item(0).unwrap(),
            _ => unreachable!(),
        };
        assert_eq!(details_item.icon_rect.width, 27.0);
        assert_eq!(details_item.text_rect.height, 27.0);
    }

    #[test]
    fn shell_hit_test_uses_content_coordinates_below_top_bar() {
        let scene = test_scene(vec![test_entry("alpha.txt", false)], ShellViewMode::Icons);
        let size = PhysicalSize::new(360, 240);
        let layout = scene.layout(size);
        let item = layout.item(0).expect("test item should layout");

        let visual_point = ViewPoint {
            x: scene.content_origin_x(size) + item.visual_rect.x + 1.0,
            y: item.visual_rect.y + scene.content_origin_y() + 1.0,
        };
        assert_eq!(scene.hit_test_screen_point(visual_point, size), Some(0));

        let top_bar_point = ViewPoint {
            x: scene.content_origin_x(size) + item.item_rect.x + 1.0,
            y: scene.content_origin_y() - 1.0,
        };
        assert_eq!(scene.hit_test_screen_point(top_bar_point, size), None);
    }

    #[test]
    fn status_bar_reserves_viewport_and_blocks_selection_hits() {
        let mut scene = test_scene(
            vec![
                test_entry("alpha.txt", false),
                test_entry("bravo.txt", false),
            ],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(360, 240);
        let status_bar = scene.status_bar_rect(size);

        assert_eq!(scene.viewport_height(size), 124.0);
        assert_eq!(status_bar.y, 204.0);
        assert_eq!(
            scene.hit_test_screen_point(
                ViewPoint {
                    x: status_bar.x + 16.0,
                    y: status_bar.y + 4.0,
                },
                size,
            ),
            None
        );

        assert!(
            scene.panes[ShellPaneId::SLOT_0]
                .selection
                .apply_navigation(0, false)
        );
        assert!(!scene.begin_pane_pointer(
            SelectionClick {
                point: ViewPoint {
                    x: status_bar.x + 16.0,
                    y: status_bar.y + 4.0,
                },
                extend: false,
                toggle: false,
            },
            size,
        ));
        assert_eq!(scene.panes[ShellPaneId::SLOT_0].selection.len(), 1);
        assert!(scene.panes[ShellPaneId::SLOT_0].selection.contains(0));
    }

    #[test]
    fn pane_status_text_is_plain_pane_state() {
        let mut scene = test_scene(
            vec![
                test_entry("folder", true),
                test_entry("alpha.txt", false),
                test_entry("bravo.txt", false),
            ],
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(520, 320);
        let projection = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();

        assert_eq!(
            scene.pane_status_text(projection.view, projection.visible_items.len()),
            "3 items, 1 folder, 2 files"
        );
        let status = scene.pane_status(projection.view, projection.visible_items.len());
        assert_eq!(status.primary, "3 items, 1 folder, 2 files");
        assert!(status.qualifier_text().is_empty());

        assert!(
            scene.panes[ShellPaneId::SLOT_0]
                .selection
                .apply_navigation(1, false)
        );
        let projection = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();
        assert_eq!(
            scene.pane_status_text(projection.view, projection.visible_items.len()),
            "1 item selected"
        );
        let status = scene.pane_status(projection.view, projection.visible_items.len());
        assert_eq!(status.primary, "1 item selected");
        assert!(status.qualifier_text().is_empty());

        scene.show_hidden = true;
        let projection = scene.pane_projection(ShellPaneId::SLOT_0, size).unwrap();
        let status = scene.pane_status(projection.view, projection.visible_items.len());
        assert_eq!(status.primary, "1 item selected");
        assert_eq!(status.qualifier_text(), "hidden shown");
    }

    #[test]
    fn task_area_uses_sidebar_bottom_without_replacing_pane_status() {
        let mut scene = test_scene(
            (0..16)
                .map(|index| test_entry(&format!("entry-{index:02}.txt"), false))
                .collect(),
            ShellViewMode::Icons,
        );
        let size = PhysicalSize::new(760, 520);
        let panel_without_tasks = scene.places_panel_rect(size);
        assert!(scene.places_task_area_rect(size).is_none());

        scene.record_task_status(ShellTaskStatus::completed("Copied", "2 item(s)", false));
        let task_area = scene
            .places_task_area_rect(size)
            .expect("recent tasks should show a sidebar task area");
        let panel_with_tasks = scene.places_panel_rect(size);
        let sidebar = scene.places_sidebar_rect(size);
        let status_bar = scene.status_bar_rect(size);

        assert!(task_area.y > panel_with_tasks.bottom());
        assert!(task_area.bottom() <= sidebar.bottom());
        assert!(panel_with_tasks.height < panel_without_tasks.height);
        assert!(status_bar.x >= sidebar.right());
    }
