use std::path::PathBuf as StagingPathBuf;

fn build_staged_text_frame(engine: &mut TextEngine) -> TextFrame {
    engine.begin_frame();
    let staging = engine.take_frame_staging();
    let mut builder = TextFrameBuilder::new_with_staging(
        TextFrameResources::from_engine(engine),
        PhysicalSize::new(320, 180),
        1.0,
        staging,
    );
    let rect = ViewRect {
        x: 8.0,
        y: 12.0,
        width: 180.0,
        height: TEXT_LINE_HEIGHT,
    };
    builder.push_label("retained label", rect, rect, TextColor::rgb(30, 40, 50));
    builder.finish()
}

fn text_frame_signature(frame: &TextFrame) -> (Vec<u8>, TextFrameStats) {
    (
        bytemuck::cast_slice::<TextVertex, u8>(&frame.vertices).to_vec(),
        frame.stats,
    )
}

#[test]
fn text_staging_reuses_warm_frame_capacity_and_output() {
    let mut engine = TextEngine::new();
    let mut first = build_staged_text_frame(&mut engine);
    let capacities = (
        first.vertices.capacity(),
        first.pixels.capacity(),
        first.uploads.capacity(),
        first.pending_draws.capacity(),
        first.drawable_indices.capacity(),
        first.atlases.capacity(),
    );
    engine.recycle_frame(&mut first);

    let mut second = build_staged_text_frame(&mut engine);
    let second_signature = text_frame_signature(&second);
    assert_eq!(second.vertices.capacity(), capacities.0);
    assert_eq!(second.pixels.capacity(), capacities.1);
    assert_eq!(second.uploads.capacity(), capacities.2);
    assert_eq!(second.pending_draws.capacity(), capacities.3);
    assert_eq!(second.drawable_indices.capacity(), capacities.4);
    assert_eq!(second.atlases.capacity(), capacities.5);
    engine.recycle_frame(&mut second);

    let third = build_staged_text_frame(&mut engine);
    assert_eq!(text_frame_signature(&third), second_signature);
}

#[test]
fn text_label_interner_reuses_owned_key_across_warm_frames() {
    let mut engine = TextEngine::new();
    let mut first = build_staged_text_frame(&mut engine);
    let first_text = Arc::clone(&first.pending_draws[0].key.text);
    let first_stats = engine.label_texts.stats();
    assert_eq!(first_stats.entries, 1);
    assert_eq!(first_stats.hits, 0);
    assert_eq!(first_stats.misses, 1);
    engine.recycle_frame(&mut first);

    let second = build_staged_text_frame(&mut engine);
    assert!(Arc::ptr_eq(&first_text, &second.pending_draws[0].key.text));
    let second_stats = engine.label_texts.stats();
    assert_eq!(second_stats.entries, 1);
    assert_eq!(second_stats.hits, 1);
    assert_eq!(second_stats.misses, 1);
}

#[test]
fn native_filename_display_cache_reuses_icons_layout_across_warm_frames() {
    let scene = test_scene(
        vec![
            test_entry("alpha-long-filename.txt", false),
            test_entry("beta-long-filename.txt", false),
        ],
        ShellViewMode::Icons,
    );
    let size = PhysicalSize::new(720, 420);
    let projection = scene
        .pane_projection(ShellPaneId::SLOT_0, size)
        .expect("icons projection");
    let visible_names = projection.visible_items.len();
    let initial_stats = scene.text_hit_tests.borrow().measure_cache_stats();
    assert_eq!(initial_stats.icons_display_hits, 0);
    assert_eq!(initial_stats.icons_display_misses, 0);

    let mut engine = TextEngine::new();
    engine.begin_frame();
    let staging = engine.take_frame_staging();
    let mut first_builder = TextFrameBuilder::new_with_staging(
        TextFrameResources::from_engine(&mut engine),
        size,
        scene.ui_scale(),
        staging,
    );
    scene.push_native_frame_text(&mut first_builder, std::slice::from_ref(&projection), size);
    let mut first = first_builder.finish();
    let first_stats = scene.text_hit_tests.borrow().measure_cache_stats();
    assert_eq!(
        first_stats.icons_display_misses - initial_stats.icons_display_misses,
        visible_names
    );
    assert_eq!(first_stats.icons_display_hits, 0);
    engine.recycle_frame(&mut first);

    engine.begin_frame();
    let staging = engine.take_frame_staging();
    let mut second_builder = TextFrameBuilder::new_with_staging(
        TextFrameResources::from_engine(&mut engine),
        size,
        scene.ui_scale(),
        staging,
    );
    scene.push_native_frame_text(&mut second_builder, std::slice::from_ref(&projection), size);
    let _second = second_builder.finish();
    let second_stats = scene.text_hit_tests.borrow().measure_cache_stats();
    assert_eq!(
        second_stats.icons_display_hits - first_stats.icons_display_hits,
        visible_names
    );
    assert_eq!(
        second_stats.icons_display_misses,
        first_stats.icons_display_misses
    );
}

#[test]
fn native_pane_status_cache_reuses_warm_labels_and_invalidates_selection() {
    let mut scene = test_scene(
        vec![
            test_entry("alpha", true),
            test_entry("beta", false),
            test_entry("gamma", false),
        ],
        ShellViewMode::Icons,
    );
    let size = PhysicalSize::new(720, 420);
    let mut engine = TextEngine::new();

    let projection = scene
        .pane_projection(ShellPaneId::SLOT_0, size)
        .expect("icons projection");
    engine.begin_frame();
    let staging = engine.take_frame_staging();
    let mut first_builder = TextFrameBuilder::new_with_staging(
        TextFrameResources::from_engine(&mut engine),
        size,
        scene.ui_scale(),
        staging,
    );
    scene.push_native_frame_text(&mut first_builder, std::slice::from_ref(&projection), size);
    let mut first = first_builder.finish();
    let first_stats = engine.pane_status_texts.stats();
    assert_eq!(first_stats.entries, 1);
    assert_eq!(first_stats.hits, 0);
    assert_eq!(first_stats.misses, 1);
    engine.recycle_frame(&mut first);

    let projection = scene
        .pane_projection(ShellPaneId::SLOT_0, size)
        .expect("icons projection");
    engine.begin_frame();
    let staging = engine.take_frame_staging();
    let mut second_builder = TextFrameBuilder::new_with_staging(
        TextFrameResources::from_engine(&mut engine),
        size,
        scene.ui_scale(),
        staging,
    );
    scene.push_native_frame_text(&mut second_builder, std::slice::from_ref(&projection), size);
    let _second = second_builder.finish();
    let second_stats = engine.pane_status_texts.stats();
    assert_eq!(second_stats.entries, 1);
    assert_eq!(second_stats.hits, 1);
    assert_eq!(second_stats.misses, 1);
    drop(projection);

    assert!(
        scene.panes[ShellPaneId::SLOT_0]
            .selection
            .apply_navigation(0, false)
    );
    let projection = scene
        .pane_projection(ShellPaneId::SLOT_0, size)
        .expect("selected icons projection");
    engine.begin_frame();
    let staging = engine.take_frame_staging();
    let mut third_builder = TextFrameBuilder::new_with_staging(
        TextFrameResources::from_engine(&mut engine),
        size,
        scene.ui_scale(),
        staging,
    );
    scene.push_native_frame_text(&mut third_builder, std::slice::from_ref(&projection), size);
    let _third = third_builder.finish();
    let third_stats = engine.pane_status_texts.stats();
    assert_eq!(third_stats.entries, 1);
    assert_eq!(third_stats.hits, 1);
    assert_eq!(third_stats.misses, 2);
}

#[test]
fn native_details_metadata_cache_reuses_size_and_modified_labels() {
    let scene = test_scene(
        vec![test_unchecked_generic_entry("payload.bin", 1536, 42)],
        ShellViewMode::Details,
    );
    let size = PhysicalSize::new(900, 420);
    let projection = scene
        .pane_projection(ShellPaneId::SLOT_0, size)
        .expect("details projection");
    let mut engine = TextEngine::new();

    engine.begin_frame();
    let staging = engine.take_frame_staging();
    let mut first_builder = TextFrameBuilder::new_with_staging(
        TextFrameResources::from_engine(&mut engine),
        size,
        scene.ui_scale(),
        staging,
    );
    scene.push_native_frame_text(&mut first_builder, std::slice::from_ref(&projection), size);
    let mut first = first_builder.finish();
    let first_stats = engine.details_texts.stats();
    assert_eq!(first_stats.entries, 2);
    assert_eq!(first_stats.hits, 0);
    assert_eq!(first_stats.misses, 2);
    engine.recycle_frame(&mut first);

    engine.begin_frame();
    let staging = engine.take_frame_staging();
    let mut second_builder = TextFrameBuilder::new_with_staging(
        TextFrameResources::from_engine(&mut engine),
        size,
        scene.ui_scale(),
        staging,
    );
    scene.push_native_frame_text(&mut second_builder, std::slice::from_ref(&projection), size);
    let _second = second_builder.finish();
    let second_stats = engine.details_texts.stats();
    assert_eq!(second_stats.entries, 2);
    assert_eq!(second_stats.hits - first_stats.hits, 2);
    assert_eq!(second_stats.misses, first_stats.misses);
}

fn build_staged_icon_frame(engine: &mut IconEngine) -> IconFrame {
    let staging = engine.take_frame_staging();
    let mut builder = IconFrameBuilder::new_with_staging(
        IconFrameResources::new(
            &mut engine.resolver,
            &mut engine.thumbnails,
            IconGpuResidentIndex::default(),
        ),
        IconFrameConfig::new(PhysicalSize::new(320, 180), 1.0, 0),
        staging,
    );
    builder.push_encoded_source(
        IconGpuSource::file(StagingPathBuf::from("/usr/share/icons/folder.svg"), 64),
        ViewRect {
            x: 12.0,
            y: 16.0,
            width: 64.0,
            height: 64.0,
        },
        IconDrawLayer::Content,
    );
    builder.finish()
}

fn icon_frame_signature(frame: &IconFrame) -> (Vec<u8>, IconFrameStats) {
    (
        bytemuck::cast_slice::<IconVertex, u8>(&frame.content_vertices).to_vec(),
        frame.stats,
    )
}

#[test]
fn icon_staging_reuses_warm_frame_capacity_and_output() {
    let mut engine = IconEngine::new();
    let mut first = build_staged_icon_frame(&mut engine);
    let capacities = (
        first.slots.capacity(),
        first.draws.capacity(),
        first.content_vertices.capacity(),
        first.content_batches.capacity(),
        first.batch_draw_indices.capacity(),
    );
    engine.recycle_frame(&mut first);

    let mut second = build_staged_icon_frame(&mut engine);
    let second_signature = icon_frame_signature(&second);
    assert_eq!(second.slots.capacity(), capacities.0);
    assert_eq!(second.draws.capacity(), capacities.1);
    assert_eq!(second.content_vertices.capacity(), capacities.2);
    assert_eq!(second.content_batches.capacity(), capacities.3);
    assert_eq!(second.batch_draw_indices.capacity(), capacities.4);
    engine.recycle_frame(&mut second);

    let third = build_staged_icon_frame(&mut engine);
    assert_eq!(icon_frame_signature(&third), second_signature);
}

#[test]
fn native_layer_staging_reuses_capacity_and_output() {
    let mut scene = test_scene(
        vec![
            test_entry("alpha.txt", false),
            test_entry("beta.txt", false),
        ],
        ShellViewMode::Icons,
    );
    let size = PhysicalSize::new(720, 420);
    let first = native_frame_layers(&mut scene, size);
    let first_base = bytemuck::cast_slice::<VulkanRectInstance, u8>(&first.base_rects).to_vec();
    let first_overlay =
        bytemuck::cast_slice::<VulkanRectInstance, u8>(&first.overlay_rects).to_vec();
    let base_capacity = first.base_rects.capacity();
    let overlay_capacity = first.overlay_rects.capacity();
    let mut retained = first;

    let mut layouts = scene.prepare_frame_projection_layouts(size);
    scene.update_visible_slot_pools_for_projection_layouts(&mut layouts);
    let projections = scene.pane_projections_from_layouts(layouts);
    scene.fill_native_frame_layers(&mut retained, size, projections.projections());

    assert_eq!(retained.base_rects.capacity(), base_capacity);
    assert_eq!(retained.overlay_rects.capacity(), overlay_capacity);
    assert_eq!(
        bytemuck::cast_slice::<VulkanRectInstance, u8>(&retained.base_rects),
        first_base.as_slice()
    );
    assert_eq!(
        bytemuck::cast_slice::<VulkanRectInstance, u8>(&retained.overlay_rects),
        first_overlay.as_slice()
    );
}

#[test]
fn folder_preview_request_staging_reuses_capacity_across_projection_refreshes() {
    let mut scene = test_scene(
        (0..24)
            .map(|index| {
                test_entry_with_mime_and_modified(
                    &format!("folder-{index:02}"),
                    true,
                    "inode/directory",
                    Some(index as u64 + 1),
                )
            })
            .collect(),
        ShellViewMode::Icons,
    );
    let size = PhysicalSize::new(720, 420);
    let mut layouts = scene.prepare_frame_projection_layouts(size);
    scene.update_visible_slot_pools_for_projection_layouts(&mut layouts);
    let projections = scene.pane_projections_from_layouts(layouts);

    scene.update_folder_preview_roles_for_projections(projections.projections());
    let first_capacity = scene.folder_preview_request_staging.borrow().capacity();
    assert!(first_capacity > 0);

    scene.update_folder_preview_roles_for_projections(projections.projections());
    assert_eq!(
        scene.folder_preview_request_staging.borrow().capacity(),
        first_capacity
    );
}

fn build_staged_projections(
    scene: &mut ShellScene,
    size: PhysicalSize<u32>,
    staging: ShellFrameProjectionStaging,
) -> (
    Vec<(ShellPaneId, Vec<ShellPaneVisibleItem>)>,
    ShellFrameProjectionStaging,
) {
    let mut layouts = scene.prepare_frame_projection_layouts_with_staging(size, staging);
    scene.update_visible_slot_pools_for_projection_layouts(&mut layouts);
    let projections = scene.pane_projections_from_layouts(layouts);
    let signature = projections
        .projections()
        .iter()
        .map(|projection| (projection.geometry.kind, projection.visible_items.clone()))
        .collect();
    (signature, projections.recycle())
}

#[test]
fn projection_staging_reuses_warm_visible_item_capacity_and_output() {
    let mut scene = test_scene(
        (0..120)
            .map(|index| test_entry(&format!("entry-{index:03}.txt"), index % 5 == 0))
            .collect(),
        ShellViewMode::Icons,
    );
    let size = PhysicalSize::new(720, 420);
    let (first_signature, staging) =
        build_staged_projections(&mut scene, size, ShellFrameProjectionStaging::default());
    let capacities = (
        staging.layouts.capacity(),
        staging.visible_items.each_ref().map(Vec::capacity),
    );
    assert!(capacities.1[ShellPaneId::SLOT_0.index()] > 0);

    let (second_signature, staging) = build_staged_projections(&mut scene, size, staging);
    assert_eq!(second_signature, first_signature);
    assert_eq!(staging.layouts.capacity(), capacities.0);
    assert_eq!(
        staging.visible_items.each_ref().map(Vec::capacity),
        capacities.1
    );
}

#[test]
fn metadata_role_rebind_reuses_prepared_projection_geometry_and_slots() {
    let mut scene = test_scene(
        vec![test_unchecked_generic_entry("payload", 12, 42)],
        ShellViewMode::Icons,
    );
    let size = PhysicalSize::new(720, 420);
    let mut layouts = scene.prepare_frame_projection_layouts(size);
    scene.update_visible_slot_pools_for_projection_layouts(&mut layouts);
    let projections = scene.pane_projections_from_layouts(layouts);
    let initial = &projections.projections()[0];
    let visible_items = initial.visible_items.clone();
    let visible_items_ptr = initial.visible_items.as_ptr();
    let visible_items_capacity = initial.visible_items.capacity();
    assert!(visible_items[0].slot_id != 0);

    let layouts = projections.into_prepared_layouts();
    assert_eq!(layouts.layouts[0].visible_items.as_ptr(), visible_items_ptr);
    assert_eq!(
        layouts.layouts[0].visible_items.capacity(),
        visible_items_capacity
    );
    assert_eq!(layouts.layouts[0].visible_items, visible_items);

    assert_eq!(
        scene.apply_synchronous_metadata_role_results(vec![MetadataRoleResult {
            pane_id: core_pane_id_for_shell_pane(ShellPaneId::SLOT_0),
            generation: Generation(0),
            item_id: shell_metadata_item_id(0),
            path: PathBuf::from("/tmp/payload"),
            role: Some(tensor_files_core::EntryMetadataRole {
                size_bytes: 12,
                modified_secs: Some(42),
                mime_type: Some(Arc::from("image/png")),
                mime_magic_checked: true,
            }),
        }]),
        1
    );

    let rebound = scene.pane_projections_from_layouts(layouts);
    let rebound = &rebound.projections()[0];
    assert_eq!(rebound.visible_items.as_ptr(), visible_items_ptr);
    assert_eq!(rebound.visible_items.capacity(), visible_items_capacity);
    assert_eq!(rebound.visible_items, visible_items);
    assert_eq!(
        rebound.view.entries[0].mime_type.as_deref(),
        Some("image/png")
    );
}

#[test]
fn icon_vertex_upload_staging_reuses_capacity_and_layer_order() {
    let vertex = |x| IconVertex {
        position: [x, x + 1.0],
        uv: [0.0, 1.0],
        rounding_bounds: [0.0; 4],
        radius_alpha: [0.0, 1.0],
    };
    let content = [vertex(1.0), vertex(2.0)];
    let overlay = [vertex(3.0)];
    let expected = [content.as_slice(), overlay.as_slice()].concat();
    let mut staging = Vec::with_capacity(8);

    crate::vulkan_icon::stage_icon_vertex_stream(&mut staging, &content, &overlay);
    assert_eq!(
        bytemuck::cast_slice::<IconVertex, u8>(&staging),
        bytemuck::cast_slice::<IconVertex, u8>(&expected)
    );
    let capacity = staging.capacity();
    crate::vulkan_icon::stage_icon_vertex_stream(&mut staging, &content, &overlay);
    assert_eq!(staging.capacity(), capacity);
}
