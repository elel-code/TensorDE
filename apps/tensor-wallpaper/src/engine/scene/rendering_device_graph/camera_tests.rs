use super::projection::scene_clip_transform_for_frame;
use super::*;
use crate::engine::scene::SceneBinaryDocument;
use crate::engine::scene::semantic_world::{
    ResolvedObjectState, ResolvedSemanticFrame, SemanticEntity,
};

#[test]
fn default_camera_still_applies_the_we_orthographic_depth_span() {
    let storage = SceneStorage::from_document(SceneBinaryDocument::default())
        .expect("default-camera storage");
    let transform = scene_clip_transform_for_frame(
        &storage,
        &ResolvedSemanticFrame::from_resolved_parts(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
        ),
        [
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.998_308_24,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
        ],
    );

    assert!((transform[2][2] - 0.000_249_577_06).abs() <= 1.0e-10);
    assert_eq!(transform[2][3], 0.5);
}

#[test]
fn authored_camera_layer_changes_scene_projection_zoom_translation_and_depth() {
    let camera_object = SceneObjectHandle(0);
    let document = crate::engine::scene::SceneBinaryDocument {
        project: SceneProjectRecord {
            logical_width: 3440,
            logical_height: 1440,
            ..crate::engine::scene::SceneBinaryDocument::default().project
        },
        objects: vec![SceneObjectRecord {
            id: camera_object,
            we_id: 420,
            name: SceneStringId::NONE,
            kind: SceneObjectKind::Camera,
            resource: SceneResourceId::NONE,
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            parent_we_id: INVALID_OBJECT_ID,
            attachment: SceneStringId::NONE,
            origin: SceneVec3 {
                x: -181.10268,
                y: -390.23996,
                z: 500.0,
            },
            angles: SceneVec3::default(),
            scale: SceneVec3::ONE,
            camera_zoom: 2.3850784,
            color: SceneVec3::ONE,
            alpha: 1.0,
            visible: true,
            color_blend_mode: 0,
            sort_order: 0,
            effect_start: INVALID_OBJECT_ID,
            effect_count: 0,
            render_graph: INVALID_OBJECT_ID,
        }],
        ..crate::engine::scene::SceneBinaryDocument::default()
    };
    let storage = SceneStorage::from_document(document).expect("camera storage");
    let camera_world = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, -181.10268, -390.23996, 500.0,
        1.0,
    ];
    let frame = ResolvedSemanticFrame::from_resolved_parts(
        vec![ResolvedObjectState {
            entity: SemanticEntity::from_raw(0),
            object: camera_object,
            object_index: 0,
            parent: SceneObjectHandle(INVALID_OBJECT_ID),
            parent_we_id: INVALID_OBJECT_ID,
            attachment: SceneStringId::NONE,
            local_matrix: camera_world,
            world_matrix: camera_world,
            render_world_matrix: camera_world,
            camera_zoom: 2.3850784,
            self_visible: true,
            resolved_visible: true,
            self_color: SceneVec3::ONE,
            resolved_color: SceneVec3::ONE,
            self_alpha: 1.0,
            resolved_alpha: 1.0,
            sort_order: 0,
            mesh_binding_start: 0,
            mesh_binding_count: 0,
            puppet_index: u32::MAX,
        }],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    let scene_center = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 1720.0, 720.0, 0.0, 1.0,
    ];

    let transform = scene_clip_transform_for_frame(&storage, &frame, scene_center);

    assert!((transform[0][0] - 2.0 * 2.3850784 / 3440.0).abs() <= 1.0e-7);
    assert!((transform[0][3] - 0.2511303).abs() <= 1.0e-6);
    assert!((transform[1][1] + 0.003312609).abs() <= 1.0e-7);
    assert!((transform[1][3] + 1.2927123).abs() <= 1.0e-6);
    assert!((transform[2][2] - 0.00025).abs() <= 1.0e-8);
    assert!((transform[2][3] - 0.375).abs() <= 1.0e-8);
}

#[test]
fn retained_camera_animation_precedes_the_current_shader_time_by_frame_delta() {
    let camera_object = SceneObjectHandle(0);
    let document = crate::engine::scene::SceneBinaryDocument {
        strings: vec!["single".to_owned()],
        objects: vec![SceneObjectRecord {
            id: camera_object,
            we_id: 1,
            name: SceneStringId::NONE,
            kind: SceneObjectKind::Camera,
            resource: SceneResourceId::NONE,
            material: SceneMaterialHandle(INVALID_MATERIAL_ID),
            parent_we_id: INVALID_OBJECT_ID,
            attachment: SceneStringId::NONE,
            origin: SceneVec3::default(),
            angles: SceneVec3::default(),
            scale: SceneVec3::ONE,
            camera_zoom: 1.0,
            color: SceneVec3::ONE,
            alpha: 1.0,
            visible: true,
            color_blend_mode: 0,
            sort_order: 0,
            effect_start: INVALID_OBJECT_ID,
            effect_count: 0,
            render_graph: INVALID_OBJECT_ID,
        }],
        object_transform_tracks: vec![SceneObjectTransformTrackRecord {
            object: camera_object,
            property: SceneObjectTransformProperty::CameraZoom,
            flags: 0,
            playback: SceneStringId(0),
            fps: 30.0,
            frame_count: 180,
            channel_start: 0,
            channel_count: 1,
        }],
        object_transform_channels: vec![SceneObjectTransformChannelRecord {
            track: 0,
            component: 0,
            kind: SceneObjectTransformChannelKind::Keyframed,
            offset: 0.0,
            amplitude: 0.0,
            frequency: 0.0,
            phase: 0.0,
            keyframe_start: 0,
            keyframe_count: 2,
        }],
        object_transform_keyframes: vec![
            SceneObjectTransformKeyframeRecord {
                frame: 0.0,
                value: 2.55,
                back: [-1.0, 0.0],
                front: [0.502_777_76, 0.0],
                flags: SCENE_OBJECT_TRANSFORM_KEYFRAME_BACK_ENABLED
                    | SCENE_OBJECT_TRANSFORM_KEYFRAME_FRONT_ENABLED,
            },
            SceneObjectTransformKeyframeRecord {
                frame: 180.0,
                value: 1.0,
                back: [-0.502_777_76, 0.0],
                front: [1.0, 0.0],
                flags: SCENE_OBJECT_TRANSFORM_KEYFRAME_BACK_ENABLED
                    | SCENE_OBJECT_TRANSFORM_KEYFRAME_FRONT_ENABLED,
            },
        ],
        ..crate::engine::scene::SceneBinaryDocument::default()
    };
    let storage = SceneStorage::from_document(document).expect("camera storage");
    let world = crate::engine::scene::SceneSemanticWorld::from_storage(&storage)
        .expect("camera semantic world");
    let mut resolver =
        crate::engine::scene::semantic_world::SemanticFrameResolver::from_world(&world)
            .expect("camera semantic resolver");

    let frame = resolver
        .resolve_frame_with_events_at(
            &world,
            1.323_461_7,
            0.25,
            &crate::engine::scene::SceneFrameEvents::default(),
        )
        .expect("camera frame");
    let camera = frame.object(camera_object).expect("resolved camera");

    assert!((camera.camera_zoom - 2.385_078_4).abs() < 2.0e-4);
}
