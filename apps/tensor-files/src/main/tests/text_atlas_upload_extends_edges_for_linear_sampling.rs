
    #[test]
fn text_atlas_upload_extends_edges_for_linear_sampling() {
        let (pixels, width, height) =
            padded_text_atlas_pixels(Arc::from(vec![10, 40, 70, 100]), 2, 2);

        assert_eq!(width, 4);
        assert_eq!(height, 4);
        assert_eq!(pixels[0], 10);
        assert_eq!(pixels[(width + 1) as usize], 10);
        assert_eq!(pixels[3], 40);
        assert_eq!(pixels[(3 * width) as usize], 70);
        assert_eq!(pixels[(3 * width + 3) as usize], 100);
}

#[test]
fn indexed_pending_text_vertices_match_direct_order() {
    let draw = PendingTextDraw {
        key: LabelCacheKey {
            text: Arc::from("entry"),
            width: 32,
            height: 12,
            alignment: LabelAlignment::Center,
            wrap: LabelWrap::None,
        },
        pixels: Arc::from([255_u8]),
        atlas_upload_required: false,
        screen: ViewRect {
            x: 4.0,
            y: 8.0,
            width: 32.0,
            height: 12.0,
        },
        rect: ViewRect {
            x: 4.0,
            y: 8.0,
            width: 32.0,
            height: 12.0,
        },
        label_width: 32,
        label_height: 12,
        color: TextColor::rgba(255, 255, 255, 255),
        layer: TextDrawLayer::Content,
    };
    let atlas = AtlasRect {
        x: 2.0,
        y: 3.0,
        width: 32.0,
        height: 12.0,
    };
    let direct = text_vertices_for_pending(
        std::slice::from_ref(&draw),
        std::slice::from_ref(&atlas),
        128,
        64,
        PhysicalSize::new(128, 64),
    );
    let indexed = text_vertices_for_pending_indices(
        std::slice::from_ref(&draw),
        &[0],
        std::slice::from_ref(&atlas),
        128,
        64,
        PhysicalSize::new(128, 64),
    );
    assert_eq!(
        bytemuck::cast_slice::<TextVertex, u8>(&direct),
        bytemuck::cast_slice::<TextVertex, u8>(&indexed)
    );
}

    #[test]
    fn text_frame_vertices_sample_inside_atlas_guard() {
        let draw = PendingTextDraw {
            key: LabelCacheKey {
                text: Arc::from("alpha"),
                width: 2,
                height: 2,
                alignment: LabelAlignment::Center,
                wrap: LabelWrap::WordOrGlyph,
            },
            pixels: Arc::from(vec![255; 4]),
            atlas_upload_required: false,
            screen: ViewRect {
                x: 0.0,
                y: 0.0,
                width: 20.0,
                height: 20.0,
            },
            rect: ViewRect {
                x: 0.0,
                y: 0.0,
                width: 20.0,
                height: 20.0,
            },
            label_width: 2,
            label_height: 2,
            color: TextColor::rgb(36, 41, 47),
            layer: TextDrawLayer::Content,
        };
        let atlas = AtlasRect {
            x: 4.0,
            y: 8.0,
            width: 4.0,
            height: 4.0,
        };

        let vertices =
            text_vertices_for_pending(&[draw], &[atlas], 64, 64, PhysicalSize::new(64, 64));
        let guard = TEXT_ATLAS_GUARD_TEXELS as f32;
        let u0 = vertices[0].uv[0] * 64.0;
        let v0 = vertices[0].uv[1] * 64.0;
        let u1 = vertices[2].uv[0] * 64.0;
        let v1 = vertices[2].uv[1] * 64.0;

        assert!((u0 - (atlas.x + guard)).abs() < 0.001);
        assert!((v0 - (atlas.y + guard)).abs() < 0.001);
        assert!((u1 - (atlas.x + guard + 2.0)).abs() < 0.001);
        assert!((v1 - (atlas.y + guard + 2.0)).abs() < 0.001);
    }

    #[test]
    fn text_atlas_upload_key_tracks_destination_dimensions_and_pixels() {
        let atlas = AtlasRect {
            x: 4.0,
            y: 8.0,
            width: 16.0,
            height: 12.0,
        };
        let first = TextAtlasUpload {
            atlas,
            pixels: Arc::from(vec![7; 16 * 12]),
            width: 16,
            height: 12,
        };
        let same = TextAtlasUpload {
            atlas,
            pixels: Arc::clone(&first.pixels),
            width: 16,
            height: 12,
        };
        let different_pixels = TextAtlasUpload {
            atlas,
            pixels: Arc::from(vec![9; 16 * 12]),
            width: 16,
            height: 12,
        };
        let different_destination = TextAtlasUpload {
            atlas: AtlasRect { x: 20.0, ..atlas },
            pixels: Arc::clone(&first.pixels),
            width: 16,
            height: 12,
        };
        let different_dimensions = TextAtlasUpload {
            atlas,
            pixels: Arc::from(vec![7; 15 * 12]),
            width: 15,
            height: 12,
        };

        assert_eq!(
            TextAtlasUploadKey::from_upload(&first),
            TextAtlasUploadKey::from_upload(&same)
        );
        assert_ne!(
            TextAtlasUploadKey::from_upload(&first),
            TextAtlasUploadKey::from_upload(&different_pixels)
        );
        assert_ne!(
            TextAtlasUploadKey::from_upload(&first),
            TextAtlasUploadKey::from_upload(&different_destination)
        );
        assert_ne!(
            TextAtlasUploadKey::from_upload(&first),
            TextAtlasUploadKey::from_upload(&different_dimensions)
        );
    }

    #[test]
    fn text_atlas_upload_skip_uses_retained_previous_frame_keys() {
        let upload = TextAtlasUpload {
            atlas: AtlasRect {
                x: 4.0,
                y: 8.0,
                width: 16.0,
                height: 12.0,
            },
            pixels: Arc::from(vec![7; 16 * 12]),
            width: 16,
            height: 12,
        };
        let changed = TextAtlasUpload {
            pixels: Arc::from(vec![9; 16 * 12]),
            ..upload.clone()
        };
        let mut last = HashSet::new();
        let mut current = HashSet::new();

        assert!(!text_atlas_upload_should_skip(&upload, &last, &mut current));
        last = current;
        let mut current = HashSet::new();
        assert!(text_atlas_upload_should_skip(&upload, &last, &mut current));
        assert!(!text_atlas_upload_should_skip(
            &changed,
            &last,
            &mut current
        ));
    }

    #[test]
    fn text_atlas_grows_from_live_content_and_reuploads_after_resize() {
        let mut font_system = FontSystem::new();
        let mut swash_cache = SwashCache::new();
        let mut text_buffer = Buffer::new_empty(Metrics::new(TEXT_FONT_SIZE, TEXT_LINE_HEIGHT));
        let mut label_cache = LabelRasterCache::new(1024 * 1024);
        let mut metrics_cache = LabelMetricsCache::new(TEXT_LABEL_METRICS_CACHE_MAX_ENTRIES);
        let mut atlas_cache = TextAtlasFrameCache::default();
        let surface_size = PhysicalSize::new(320, 240);
        let clip = ViewRect {
            x: 0.0,
            y: 0.0,
            width: 320.0,
            height: 240.0,
        };
        let label_rect = ViewRect {
            x: 0.0,
            y: 0.0,
            width: 180.0,
            height: TEXT_LINE_HEIGHT,
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
            surface_size,
            1.0,
            Vec::new(),
        );
        first.push_label(
            "label-0",
            label_rect,
            clip,
            TextColor::rgb(36, 41, 47),
        );
        let first_frame = first.finish();

        assert!(first_frame.height < TEXT_ATLAS_MAX_HEIGHT);
        assert_eq!(first_frame.uploads.len(), 1);
        let first_height = first_frame.height;

        let mut second = TextFrameBuilder::new(
            TextFrameResources::new(
                &mut font_system,
                &mut swash_cache,
                &mut text_buffer,
                &mut label_cache,
                &mut metrics_cache,
                &mut atlas_cache,
            ),
            surface_size,
            1.0,
            Vec::new(),
        );
        let label_count = 256;
        for index in 0..label_count {
            second.push_label(
                &format!("label-{index}"),
                label_rect,
                clip,
                TextColor::rgb(36, 41, 47),
            );
        }
        let second_frame = second.finish();

        assert!(second_frame.height > first_height);
        assert_eq!(second_frame.stats.quads, label_count);
        assert_eq!(second_frame.uploads.len(), label_count);
        assert_eq!(second_frame.stats.atlas_reused, 0);
    }

    #[test]
    fn start_no_wrap_labels_rasterize_to_shaped_text_width() {
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
        text.push_label_aligned_no_wrap(
            "a",
            ViewRect {
                x: 0.0,
                y: 0.0,
                width: 220.0,
                height: TEXT_LINE_HEIGHT,
            },
            ViewRect {
                x: 0.0,
                y: 0.0,
                width: 240.0,
                height: 120.0,
            },
            TextColor::rgb(36, 41, 47),
            LabelAlignment::Start,
        );
        let frame = text.finish();

        assert_eq!(frame.stats.quads, 1);
        assert!(frame.stats.cache_bytes < (220.0 * TEXT_LINE_HEIGHT * 4.0) as usize);
    }

    #[test]
    fn start_no_wrap_labels_reuse_natural_width_across_rect_widths() {
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
        let clip = ViewRect {
            x: 0.0,
            y: 0.0,
            width: 260.0,
            height: 120.0,
        };
        text.push_label_aligned_no_wrap(
            "alpha",
            ViewRect {
                x: 0.0,
                y: 0.0,
                width: 220.0,
                height: TEXT_LINE_HEIGHT,
            },
            clip,
            TextColor::rgb(36, 41, 47),
            LabelAlignment::Start,
        );
        text.push_label_aligned_no_wrap(
            "alpha",
            ViewRect {
                x: 0.0,
                y: 24.0,
                width: 160.0,
                height: TEXT_LINE_HEIGHT,
            },
            clip,
            TextColor::rgb(36, 41, 47),
            LabelAlignment::Start,
        );
        let frame = text.finish();

        assert_eq!(frame.stats.quads, 2);
        assert_eq!(frame.stats.cache_misses, 1);
        assert_eq!(frame.stats.cache_hits, 1);
        assert_eq!(frame.stats.cache_entries, 1);
    }

    #[test]
    fn location_bar_keeps_full_width_hit_target_when_editing() {
        let mut scene = test_scene(vec![test_entry("alpha", false)], ShellViewMode::Icons);
        scene.panes[ShellPaneId::SLOT_0].path = PathBuf::from("/x");
        let size = PhysicalSize::new(900, 360);

        let inactive = scene
            .path_bar_rect(size)
            .expect("inactive path bar should be visible");
        assert!(scene.apply_location_command(LocationCommand::Activate, size));
        let active = scene
            .path_bar_rect(size)
            .expect("active path bar should be visible");

        assert_eq!(active.width, inactive.width);
        assert_eq!(active.height, 28.0);
        assert!(scene.path_bar_contains_screen_point(
            ViewPoint {
                x: active.right() - 2.0,
                y: active.y + 2.0,
            },
            size
        ));
    }

    #[test]
    fn zoom_shortcuts_accept_common_characters() {
        assert_eq!(
            zoom_action_for_key(&Key::Character("+".into())),
            Some(ZoomAction::In)
        );
        assert_eq!(
            zoom_action_for_key(&Key::Character("=".into())),
            Some(ZoomAction::In)
        );
        assert_eq!(
            zoom_action_for_key(&Key::Character("-".into())),
            Some(ZoomAction::Out)
        );
        assert_eq!(
            zoom_action_for_key(&Key::Character("0".into())),
            Some(ZoomAction::Reset)
        );
        assert_eq!(zoom_action_for_key(&Key::Character("x".into())), None);
        assert_eq!(zoom_action_for_scroll_delta(-1.0), Some(ZoomAction::In));
        assert_eq!(zoom_action_for_scroll_delta(1.0), Some(ZoomAction::Out));
        assert_eq!(zoom_action_for_scroll_delta(0.0), None);
    }

    #[test]
    fn selection_shortcuts_accept_ctrl_a_and_escape() {
        let unidentified = PhysicalKey::Unidentified(NativeKeyCode::Unidentified);
        let no_key = Key::Unidentified(NativeKey::Unidentified);

        assert_eq!(
            selection_command_for_key_parts(
                true,
                &PhysicalKey::Code(KeyCode::KeyA),
                &no_key,
                &no_key,
            ),
            Some(SelectionCommand::SelectAll)
        );
        assert_eq!(
            selection_command_for_key_parts(
                true,
                &unidentified,
                &Key::Character("A".into()),
                &no_key,
            ),
            Some(SelectionCommand::SelectAll)
        );
        assert_eq!(
            selection_command_for_key_parts(
                false,
                &PhysicalKey::Code(KeyCode::KeyA),
                &Key::Character("a".into()),
                &Key::Character("a".into()),
            ),
            None
        );
        assert_eq!(
            selection_command_for_key_parts(
                false,
                &PhysicalKey::Code(KeyCode::Escape),
                &no_key,
                &no_key,
            ),
            Some(SelectionCommand::Clear)
        );
    }

    #[test]
    fn filter_shortcuts_activate_and_capture_text_when_active() {
        let unidentified = PhysicalKey::Unidentified(NativeKeyCode::Unidentified);
        let no_key = Key::Unidentified(NativeKey::Unidentified);

        assert_eq!(
            filter_command_for_key_parts(
                true,
                false,
                &PhysicalKey::Code(KeyCode::KeyF),
                &no_key,
                &no_key,
            ),
            Some(FilterCommand::Activate)
        );
        assert_eq!(
            filter_command_for_key_parts(
                false,
                true,
                &unidentified,
                &Key::Character("1".into()),
                &no_key,
            ),
            Some(FilterCommand::Insert("1".to_string()))
        );
        assert_eq!(
            filter_command_for_key_parts(
                false,
                true,
                &PhysicalKey::Code(KeyCode::Backspace),
                &no_key,
                &no_key,
            ),
            Some(FilterCommand::Backspace)
        );
        assert_eq!(
            filter_command_for_key_parts(
                false,
                true,
                &PhysicalKey::Code(KeyCode::Escape),
                &no_key,
                &no_key,
            ),
            Some(FilterCommand::ClearAndDeactivate)
        );
        assert_eq!(
            filter_command_for_key_parts(
                false,
                false,
                &unidentified,
                &Key::Character("a".into()),
                &no_key,
            ),
            None
        );
    }

    #[test]
    fn create_dialog_key_input_captures_text_and_commit_controls() {
        let unidentified = PhysicalKey::Unidentified(NativeKeyCode::Unidentified);
        let no_key = Key::Unidentified(NativeKey::Unidentified);

        assert_eq!(
            create_command_for_key_parts(
                false,
                &unidentified,
                &Key::Character("x".into()),
                &no_key,
            ),
            CreateCommand::Insert("x".to_string())
        );
        assert_eq!(
            create_command_for_key_parts(
                false,
                &PhysicalKey::Code(KeyCode::Backspace),
                &no_key,
                &no_key,
            ),
            CreateCommand::Backspace
        );
        assert_eq!(
            create_command_for_key_parts(
                false,
                &unidentified,
                &Key::Named(NamedKey::Enter),
                &no_key,
            ),
            CreateCommand::Commit
        );
        assert_eq!(
            create_command_for_key_parts(
                false,
                &PhysicalKey::Code(KeyCode::Escape),
                &no_key,
                &no_key,
            ),
            CreateCommand::Cancel
        );
        assert_eq!(
            create_command_for_key_parts(
                true,
                &PhysicalKey::Code(KeyCode::KeyA),
                &Key::Character("a".into()),
                &Key::Character("a".into()),
            ),
            CreateCommand::Ignore
        );
    }
