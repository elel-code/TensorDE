use super::*;

#[test]
fn pointer_parallax_propagates_root_depth_and_retains_position_after_leave() {
    let mut document = semantic_document();
    document.project.logical_width = 100;
    document.project.logical_height = 100;
    document.project.camera_eye = SceneVec3 {
        x: 5.0,
        y: 7.0,
        z: 0.0,
    };
    document.camera_parallax = SceneCameraParallaxRecord {
        enabled: true,
        amount: 0.5,
        delay: 0.1,
        mouse_influence: 1.0,
    };
    document.object_parallax_depths = vec![
        SceneObjectParallaxDepthRecord {
            object: SceneObjectHandle(0),
            depth: [-0.1, -0.2],
        },
        SceneObjectParallaxDepthRecord {
            object: SceneObjectHandle(1),
            depth: [-0.9, -0.8],
        },
    ];
    let storage = SceneStorage::from_document(document).expect("storage");
    let world = SceneSemanticWorld::from_storage(&storage).expect("semantic world");
    let mut resolver = SemanticFrameResolver::from_world(&world).expect("resolver");
    let mut events = crate::engine::scene::SceneFrameEvents {
        pointer: crate::engine::scene::ScenePointerState {
            source: crate::engine::scene::ScenePointerSource::Replay,
            position: [100.0, 0.0],
            surface_size: [100, 100],
            inside: true,
            ..crate::engine::scene::ScenePointerState::default()
        },
        ..crate::engine::scene::SceneFrameEvents::default()
    };

    resolver
        .resolve_frame_with_events_at(&world, 7.0, 0.0, &events)
        .expect("first pointer frame");
    let frame = resolver
        .resolve_frame_with_events_at(&world, 7.0, 0.1, &events)
        .expect("pointer response frame");
    let root = frame.object(SceneObjectHandle(0)).expect("root image");
    let child = frame.object(SceneObjectHandle(1)).expect("child puppet");
    assert_close(root.world_matrix[12], 10.0);
    assert_close(root.world_matrix[13], 20.0);
    assert_close(root.render_world_matrix[12], 14.666666);
    assert_close(root.render_world_matrix[13], 28.533333);
    assert_close(
        child.render_world_matrix[12] - child.world_matrix[12],
        root.render_world_matrix[12] - root.world_matrix[12],
    );
    assert_close(
        child.render_world_matrix[13] - child.world_matrix[13],
        root.render_world_matrix[13] - root.world_matrix[13],
    );

    events.pointer.inside = false;
    let frame = resolver
        .resolve_frame_with_events_at(&world, 7.0, 1.0, &events)
        .expect("pointer leave frame");
    let root = frame.object(SceneObjectHandle(0)).expect("root image");
    // Protocol focus loss must not synthesize a desktop-pointer recenter.
    assert_close(root.render_world_matrix[12], 14.75);
    assert_close(root.render_world_matrix[13], 28.7);
}

#[test]
fn pointer_parallax_zero_delay_reaches_target_on_zero_delta_frame() {
    let mut document = semantic_document();
    document.project.logical_width = 100;
    document.project.logical_height = 100;
    document.project.camera_eye = SceneVec3::default();
    document.camera_parallax = SceneCameraParallaxRecord {
        enabled: true,
        amount: 0.5,
        delay: 0.0,
        mouse_influence: 1.0,
    };
    document.object_parallax_depths = vec![SceneObjectParallaxDepthRecord {
        object: SceneObjectHandle(0),
        depth: [-0.1, -0.2],
    }];
    let storage = SceneStorage::from_document(document).expect("storage");
    let world = SceneSemanticWorld::from_storage(&storage).expect("semantic world");
    let mut resolver = SemanticFrameResolver::from_world(&world).expect("resolver");
    let events = crate::engine::scene::SceneFrameEvents {
        pointer: crate::engine::scene::ScenePointerState {
            source: crate::engine::scene::ScenePointerSource::Replay,
            position: [100.0, 0.0],
            surface_size: [100, 100],
            inside: true,
            ..crate::engine::scene::ScenePointerState::default()
        },
        ..crate::engine::scene::SceneFrameEvents::default()
    };

    let frame = resolver
        .resolve_frame_with_events_at(&world, 0.0, 0.0, &events)
        .expect("zero-delay first pointer frame");
    let root = frame.object(SceneObjectHandle(0)).expect("root image");
    assert_close(root.world_matrix[12], 10.0);
    assert_close(root.world_matrix[13], 20.0);
    assert_close(root.render_world_matrix[12], 14.5);
    assert_close(root.render_world_matrix[13], 28.0);
}
