fn native_frame_layers(
    scene: &mut ShellScene,
    size: PhysicalSize<u32>,
) -> crate::vulkan_rect::NativeFrameLayers {
    let mut layouts = scene.prepare_frame_projection_layouts(size);
    scene.update_visible_slot_pools_for_projection_layouts(&mut layouts);
    let projections = scene.pane_projections_from_layouts(layouts);
    scene.build_native_frame_layers(size, projections.projections())
}

fn native_text_frame(scene: &mut ShellScene, size: PhysicalSize<u32>) -> TextFrame {
    let mut layouts = scene.prepare_frame_projection_layouts(size);
    scene.update_visible_slot_pools_for_projection_layouts(&mut layouts);
    let projections = scene.pane_projections_from_layouts(layouts);
    let mut engine = TextEngine::new();
    engine.begin_frame();
    let mut text = TextFrameBuilder::new(
        TextFrameResources::from_engine(&mut engine),
        size,
        scene.ui_scale(),
        Vec::new(),
    );
    scene.push_native_frame_text(&mut text, projections.projections(), size);
    text.finish()
}

#[test]
fn native_frame_layers_keep_structural_and_interaction_chrome_analytic() {
    let mut scene = test_scene(
        vec![test_entry("alpha.txt", false), test_entry("beta.txt", false)],
        ShellViewMode::Icons,
    );
    let size = PhysicalSize::new(720, 420);

    let plain = native_frame_layers(&mut scene, size);
    assert!(plain.base_rects.len() >= 6);

    assert!(scene.panes[ShellPaneId::SLOT_0]
        .selection
        .apply_navigation(0, false));
    let selected = native_frame_layers(&mut scene, size);
    assert!(selected.base_rects.len() > plain.base_rects.len());
    assert!(selected
        .base_rects
        .iter()
        .any(|instance| instance.color() == [0.239, 0.502, 0.710, 0.32]));

    scene.rubber_band = Some(RubberBand {
        start: ViewPoint { x: 16.0, y: 20.0 },
        current: ViewPoint { x: 160.0, y: 108.0 },
        active: true,
        mode: RubberBandMode::Replace,
        base_selection: ShellSelection::default(),
    });
    let with_rubber_band = native_frame_layers(&mut scene, size);
    assert!(with_rubber_band.base_rects.len() > selected.base_rects.len());
    assert!(with_rubber_band
        .base_rects
        .iter()
        .any(|instance| instance.color() == [0.280, 0.580, 0.920, 0.18]));
    assert!(with_rubber_band
        .base_rects
        .iter()
        .any(|instance| instance.color() == [0.450, 0.720, 0.980, 0.92]));
}

#[test]
fn native_frame_layers_encode_filter_and_details_chrome_as_instances() {
    let size = PhysicalSize::new(720, 420);
    let mut icons = test_scene(Vec::new(), ShellViewMode::Icons);
    let plain = native_frame_layers(&mut icons, size);

    icons.filter_active = true;
    let filtered = native_frame_layers(&mut icons, size);
    assert!(filtered.base_rects.len() >= plain.base_rects.len() + 3);

    let mut details = test_scene(Vec::new(), ShellViewMode::Details);
    let details_layers = native_frame_layers(&mut details, size);
    assert!(details_layers.base_rects.len() >= plain.base_rects.len() + 2);
}

#[test]
fn native_text_atlas_uses_retained_shell_label_recipes_without_cpu_quads() {
    let mut scene = test_scene(
        vec![test_entry("alpha.txt", false), test_entry("beta.txt", false)],
        ShellViewMode::Icons,
    );
    let size = PhysicalSize::new(720, 420);
    let frame = native_text_frame(&mut scene, size);

    assert!(frame.stats.labels > 2);
    assert_eq!(frame.vertices.len(), frame.stats.quads * 6);
    assert!(!frame.uploads.is_empty());
}

#[test]
fn native_text_atlas_tracks_places_filter_and_details_recipes() {
    let size = PhysicalSize::new(720, 420);
    let mut icons = test_scene(Vec::new(), ShellViewMode::Icons);
    let with_places = native_text_frame(&mut icons, size).stats.labels;

    icons.places_visible = false;
    let without_places = native_text_frame(&mut icons, size).stats.labels;
    assert!(with_places > without_places);

    icons.filter_active = true;
    let with_filter = native_text_frame(&mut icons, size).stats.labels;
    assert!(with_filter > without_places);

    let mut details = TestShellSceneBuilder::new()
        .with_places_visible(false)
        .with_view_mode(ShellViewMode::Details)
        .build();
    let with_details = native_text_frame(&mut details, size).stats.labels;
    assert!(with_details >= without_places + 3);
}
