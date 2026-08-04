use super::*;
use crate::engine::scene::{
    SceneBinaryDocument, SceneCullMode, SceneDepthTest, SceneMaterialConstantRecord,
    SceneMaterialPassRecord, SceneMaterialRecord, ScenePipelineBlend,
    SceneRenderingDeviceDrawPrimitive, SceneResourceId, SceneStringId,
};

#[test]
fn ordinary_draw_uniform_preserves_clip_matrix() {
    let storage = SceneStorage::from_document(SceneBinaryDocument::default()).expect("storage");
    let mut draw = draw_with_material(SceneMaterialHandle(INVALID_MATERIAL_ID));
    draw.clip_transform = [
        [1.0, 2.0, 3.0, 4.0],
        [5.0, 6.0, 7.0, 8.0],
        [9.0, 10.0, 11.0, 12.0],
        [13.0, 14.0, 15.0, 16.0],
    ];

    let payload = pack_scene_draw_uniforms(&storage, &[draw], 9.0, [1, 1]);

    assert_eq!(payload.len(), SCENE_DRAW_UNIFORM_BYTES as usize);
    assert_eq!(payload_f32(&payload, 0), 1.0);
    assert_eq!(payload_f32(&payload, 60), 16.0);
}
#[test]
fn scene_domain_ordinary_draw_uniform_applies_output_cover_scale() {
    let mut document = SceneBinaryDocument::default();
    document.project.logical_width = 3_840;
    document.project.logical_height = 2_160;
    let storage = SceneStorage::from_document(document).expect("storage");
    let mut draw = draw_with_material(SceneMaterialHandle(INVALID_MATERIAL_ID));
    draw.clip_transform = [
        [2.0 / 415.0, 0.0, 0.0, -1.0],
        [0.0, 2.0 / 405.0, 0.0, -1.0],
        [0.0, 0.0, 0.0005, 0.5],
        [0.0, 0.0, 0.0, 1.0],
    ];

    let payload = pack_scene_draw_uniforms(&storage, &[draw], 0.0, [3_856, 2_199]);

    assert_close(payload_f32(&payload, 0), 0.004_885_934);
    assert_close(payload_f32(&payload, 12), -1.013_831_4);
    assert_close(payload_f32(&payload, 20), 2.0 / 405.0);
}

#[test]
fn authored_texture_ordinary_draw_uniform_bypasses_output_cover_scale() {
    let mut document = SceneBinaryDocument::default();
    document.project.logical_width = 3_840;
    document.project.logical_height = 2_160;
    let storage = SceneStorage::from_document(document).expect("storage");
    let mut draw = draw_with_material(SceneMaterialHandle(INVALID_MATERIAL_ID));
    draw.projection_domain =
        crate::engine::scene::SceneRenderingDeviceProjectionDomain::AuthoredTexture {
            width: 415,
            height: 405,
        };
    draw.clip_transform = [
        [2.0 / 415.0, 0.0, 0.0, 0.0],
        [0.0, -2.0 / 405.0, 0.0, 0.0],
        [0.0, 0.0, 0.0005, 0.5],
        [0.0, 0.0, 0.0, 1.0],
    ];

    let payload = pack_scene_draw_uniforms(&storage, &[draw], 0.0, [3_856, 2_199]);

    assert_eq!(payload_f32(&payload, 0), 2.0 / 415.0);
    assert_eq!(payload_f32(&payload, 12), 0.0);
    assert_eq!(payload_f32(&payload, 20), -2.0 / 405.0);
}

#[test]
fn depth_parallax_draw_uniform_packs_inverse_effect_texture_projection() {
    let mut document = audio_bars_storage().document().clone();
    document.strings[0] = "effects/depthparallax__SLOTS_3__QUALITY_2".to_owned();
    let storage = SceneStorage::from_document(document).expect("Depth Parallax storage");
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.primitive = SceneRenderingDeviceDrawPrimitive::FullscreenTriangle;
    draw.clip_transform = [
        [2.0, 0.25, 0.0, 0.5],
        [0.0, -4.0, 0.0, -0.25],
        [0.0, 0.0, 0.5, 0.125],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let projection = draw_projection_matrix(&storage, &draw, [1, 1]);
    let expected = inverse_affine_rows(&projection).expect("invertible projection");

    let payload = pack_scene_draw_uniforms(&storage, &[draw], 0.0, [1, 1]);

    for (lane, expected) in expected.into_iter().flatten().enumerate() {
        assert_close(payload_f32(&payload, lane * size_of::<f32>()), expected);
    }
}

#[test]
fn object_mesh_depth_parallax_draw_uniform_packs_authored_position_mvp() {
    let mut document = audio_bars_storage().document().clone();
    document.strings[0] = "effects/depthparallax__SLOTS_3__QUALITY_2".to_owned();
    let storage = SceneStorage::from_document(document).expect("Depth Parallax storage");
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.primitive = SceneRenderingDeviceDrawPrimitive::ObjectMesh;
    draw.clip_transform = [
        [2.0, 0.25, 0.0, 0.5],
        [0.0, -4.0, 0.0, -0.25],
        [0.0, 0.0, 0.5, 0.125],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let expected = matrix_draw_values(draw_projection_matrix(&storage, &draw, [1, 1]));

    let payload = pack_scene_draw_uniforms(&storage, &[draw], 0.0, [1, 1]);

    for (lane, expected) in expected.into_iter().enumerate() {
        assert_close(payload_f32(&payload, lane * size_of::<f32>()), expected);
    }
}

#[test]
fn iris_draw_uniform_maps_named_constants_and_time() {
    let storage = iris_storage();
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.primitive = SceneRenderingDeviceDrawPrimitive::FullscreenTriangle;
    let payload = pack_scene_draw_uniforms(&storage, &[draw], 3.25, [1, 1]);

    assert_eq!(payload_f32(&payload, 0), 3.25);
    assert_eq!(payload_f32(&payload, 4), 1.5);
    assert_eq!(payload_f32(&payload, 8), 0.35);
    assert_eq!(payload_f32(&payload, 12), 0.75);
    assert_eq!(payload_f32(&payload, 16), 2.0);
    assert_eq!(payload_f32(&payload, 20), 3.0);
    assert_eq!(payload_f32(&payload, 24), 0.4);
    assert_eq!(payload_f32(&payload, 28), 1.0);
}

#[test]
fn oscilloscope_object_mesh_terminal_uses_authored_position_mvp() {
    let storage = oscilloscope_storage();
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.primitive = SceneRenderingDeviceDrawPrimitive::ObjectMesh;
    draw.authored_source_extent = [20.0, 40.0];
    draw.clip_transform = [
        [0.1, 0.0, 0.0, 0.25],
        [0.0, -0.05, 0.0, -0.125],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let expected = matrix_draw_values(scene_cover_clip_transform(
        storage.project(),
        [100, 100],
        draw.clip_transform,
    ));

    let payload = pack_scene_draw_uniforms(&storage, &[draw], 0.0, [100, 100]);

    for (lane, expected) in expected.into_iter().enumerate() {
        assert_close(payload_f32(&payload, lane * size_of::<f32>()), expected);
    }
    assert_close(payload_f32(&payload, 12), 0.25);
    assert_close(payload_f32(&payload, 28), -0.125);
}

#[test]
fn object_composite_maps_screen_uv_back_to_object_uv() {
    let mut document = audio_bars_storage().document().clone();
    document.strings.push("we/objectcomposite".to_owned());
    let shader_key = crate::engine::scene::SceneStringId((document.strings.len() - 1) as u32);
    let storage = SceneStorage::from_document(document).expect("object composite storage");
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.shader_key = shader_key;
    draw.primitive = SceneRenderingDeviceDrawPrimitive::FullscreenTriangle;
    draw.authored_source_extent = [20.0, 40.0];
    draw.clip_transform = [
        [0.1, 0.0, 0.0, 0.0],
        [0.0, -0.05, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let expected = projected_object_uv_draw_values(&storage, &draw, [100, 100]);

    let payload = pack_scene_draw_uniforms(&storage, &[draw], 0.0, [100, 100]);

    for (lane, expected) in expected.into_iter().enumerate() {
        assert_close(payload_f32(&payload, lane * size_of::<f32>()), expected);
    }
}

#[test]
fn quantized_framebuffer_water_shake_uses_projected_object_uv_rows() {
    assert_shader_uses_projected_object_uv("we/framebuffer-water-quantized-shake-final");
}

#[test]
fn quantized_caustics_prepass_uses_projected_object_uv_rows() {
    assert_shader_uses_projected_object_uv(
        "effects/caustics__SLOTS_3d__BLENDMODE_6__TENSOR_WALLPAPER_FRAMEBUFFER_QUANTIZED_OVERLAY_1",
    );
}

fn assert_shader_uses_projected_object_uv(shader: &str) {
    let mut document = audio_bars_storage().document().clone();
    document.strings[0] = shader.to_owned();
    let storage = SceneStorage::from_document(document).expect("projected shader storage");
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.shader_key = SceneStringId(0);
    draw.authored_source_extent = [20.0, 40.0];
    draw.clip_transform = [
        [0.1, 0.0, 0.0, 0.25],
        [0.0, -0.05, 0.0, -0.125],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let expected = projected_object_uv_draw_values(&storage, &draw, [100, 100]);

    let payload = pack_scene_draw_uniforms(&storage, &[draw], 0.0, [100, 100]);

    for (lane, expected) in expected.into_iter().enumerate() {
        assert_close(payload_f32(&payload, lane * size_of::<f32>()), expected);
    }
}

#[test]
fn waterwaves_keeps_phase_in_object_uv_when_only_translation_differs() {
    let storage = waterwaves_storage();
    let mut shadow = draw_with_material(SceneMaterialHandle(0));
    shadow.primitive = SceneRenderingDeviceDrawPrimitive::FullscreenTriangle;
    shadow.authored_source_extent = [1571.0, 2621.0];
    shadow.clip_transform = [
        [0.0005609375, 0.0, 0.0, 0.022509336],
        [0.0, -0.0009972223, 0.0, -0.033291817],
        [0.0, 0.0, 1.077, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let mut body = shadow;
    body.clip_transform[0][3] = 0.03257537;
    body.clip_transform[1][3] = -0.04262042;

    let payload = pack_scene_draw_uniforms(&storage, &[shadow, body], 0.0, [1, 1]);
    let authored_uv = [0.83, 0.42];
    let shadow_screen_uv = packed_affine_point(&payload, 0, 8, 12, authored_uv);
    let body_screen_uv = packed_affine_point(&payload, 1, 8, 12, authored_uv);
    let shadow_recovered_uv = packed_affine_point(&payload, 0, 0, 4, shadow_screen_uv);
    let body_recovered_uv = packed_affine_point(&payload, 1, 0, 4, body_screen_uv);

    assert!((shadow_screen_uv[0] - body_screen_uv[0]).abs() > 0.001);
    assert!((shadow_screen_uv[1] - body_screen_uv[1]).abs() > 0.001);
    assert_vec2_close(shadow_recovered_uv, authored_uv);
    assert_vec2_close(body_recovered_uv, authored_uv);
    let direction = [0.6_f32, 0.8_f32];
    let shadow_phase_position =
        shadow_recovered_uv[0] * direction[0] + shadow_recovered_uv[1] * direction[1];
    let body_phase_position =
        body_recovered_uv[0] * direction[0] + body_recovered_uv[1] * direction[1];
    assert_close(shadow_phase_position, body_phase_position);
    assert_close(
        payload_f32(&payload, 32),
        0.5 * shadow.clip_transform[0][0] * shadow.authored_source_extent[0],
    );
    assert_close(payload_f32(&payload, 32), payload_f32(&payload, 96));
    assert_close(payload_f32(&payload, 52), payload_f32(&payload, 116));
}

#[test]
fn projected_pixel_extent_uses_screen_to_object_gradient() {
    let affine = [
        0.5, 0.0, 0.0, 0.0, 0.0, 0.25, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 4.0, 0.0, 0.0,
    ];
    assert_eq!(
        projected_object_pixel_extent(affine, [100, 80]),
        Some([200.0, 320.0])
    );
}

fn iris_storage() -> SceneStorage {
    SceneStorage::from_document(SceneBinaryDocument {
        strings: vec![
            "effects/iris__SLOTS_3__MASK_1".to_owned(),
            "scale".to_owned(),
            "\"2 3\"".to_owned(),
            "speed".to_owned(),
            "1.5".to_owned(),
            "rough".to_owned(),
            "0.35".to_owned(),
            "noiseamount".to_owned(),
            "0.75".to_owned(),
            "phase".to_owned(),
            "0.4".to_owned(),
        ],
        materials: vec![SceneMaterialRecord {
            id: SceneMaterialHandle(0),
            resource: SceneResourceId::NONE,
            pass_start: 0,
            pass_count: 1,
        }],
        material_passes: vec![SceneMaterialPassRecord {
            material: SceneMaterialHandle(0),
            shader_key: SceneStringId(0),
            target: SceneStringId::NONE,
            texture_start: 0,
            texture_count: 0,
            constant_start: 0,
            constant_count: 5,
            pipeline_blend: ScenePipelineBlend::Normal,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            alpha_writing: SceneStringId::NONE,
            clear_target: false,
        }],
        material_constants: (0..5)
            .map(|index| SceneMaterialConstantRecord {
                name: SceneStringId(1 + index * 2),
                value_json: SceneStringId(2 + index * 2),
            })
            .collect(),
        ..SceneBinaryDocument::default()
    })
    .expect("storage")
}

fn waterwaves_storage() -> SceneStorage {
    SceneStorage::from_document(SceneBinaryDocument {
        strings: vec!["effects/waterwaves__SLOTS_3".to_owned()],
        materials: vec![SceneMaterialRecord {
            id: SceneMaterialHandle(0),
            resource: SceneResourceId::NONE,
            pass_start: 0,
            pass_count: 1,
        }],
        material_passes: vec![SceneMaterialPassRecord {
            material: SceneMaterialHandle(0),
            shader_key: SceneStringId(0),
            target: SceneStringId::NONE,
            texture_start: 0,
            texture_count: 0,
            constant_start: 0,
            constant_count: 0,
            pipeline_blend: ScenePipelineBlend::Normal,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            alpha_writing: SceneStringId::NONE,
            clear_target: false,
        }],
        ..SceneBinaryDocument::default()
    })
    .expect("waterwaves storage")
}

fn audio_bars_storage() -> SceneStorage {
    SceneStorage::from_document(SceneBinaryDocument {
        strings: vec!["effects/simple_audio_bars__SLOTS_1__SHAPE_7".to_owned()],
        materials: vec![SceneMaterialRecord {
            id: SceneMaterialHandle(0),
            resource: SceneResourceId::NONE,
            pass_start: 0,
            pass_count: 1,
        }],
        material_passes: vec![SceneMaterialPassRecord {
            material: SceneMaterialHandle(0),
            shader_key: SceneStringId(0),
            target: SceneStringId::NONE,
            texture_start: 0,
            texture_count: 0,
            constant_start: 0,
            constant_count: 0,
            pipeline_blend: ScenePipelineBlend::Normal,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            alpha_writing: SceneStringId::NONE,
            clear_target: false,
        }],
        ..SceneBinaryDocument::default()
    })
    .expect("audio bars storage")
}

fn oscilloscope_storage() -> SceneStorage {
    SceneStorage::from_document(SceneBinaryDocument {
        strings: vec!["effects/audio_responsive_oscilloscope__SLOTS_5__RESOLUTION_16".to_owned()],
        materials: vec![SceneMaterialRecord {
            id: SceneMaterialHandle(0),
            resource: SceneResourceId::NONE,
            pass_start: 0,
            pass_count: 1,
        }],
        material_passes: vec![SceneMaterialPassRecord {
            material: SceneMaterialHandle(0),
            shader_key: SceneStringId(0),
            target: SceneStringId::NONE,
            texture_start: 0,
            texture_count: 0,
            constant_start: 0,
            constant_count: 0,
            pipeline_blend: ScenePipelineBlend::Normal,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            alpha_writing: SceneStringId::NONE,
            clear_target: false,
        }],
        ..SceneBinaryDocument::default()
    })
    .expect("oscilloscope storage")
}

fn draw_with_material(material: SceneMaterialHandle) -> SceneRenderingDeviceMeshDraw {
    SceneRenderingDeviceMeshDraw {
        primitive: SceneRenderingDeviceDrawPrimitive::ObjectMesh,
        particle_index: crate::engine::scene::INVALID_PARTICLE_INDEX,
        projection_domain: crate::engine::scene::SceneRenderingDeviceProjectionDomain::Scene,
        shader_key: crate::engine::scene::SceneStringId::NONE,
        mesh_index: 0,
        resolved_object_index: 0,
        render_world_matrix: [[0.0; 4]; 4],
        clip_transform: [[0.0; 4]; 4],
        effect_model_view_projection_matrix: [[0.0; 4]; 4],
        authored_source_extent: [0.0; 2],
        uv_inset_texels: 0.0,
        skinning_palette_start: crate::engine::scene::INVALID_OBJECT_ID,
        skinning_palette_count: 0,
        resolved_color: crate::engine::scene::SceneVec3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
        },
        resolved_alpha: 1.0,
        apply_resolved_visual: true,
        effect_batch_atlas_tile: u32::MAX,
        effect_batch_atlas_grid: [0; 2],
        effect_binding_start: u32::MAX,
        effect_binding_count: 0,
        effect_visibility_policy: crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
        resolved_effect_visibility_mask: 0,
        object: crate::engine::scene::SceneObjectHandle(0),
        material,
        vertex_start: 0,
        vertex_count: 4,
        index_start: 0,
        index_count: 6,
        instance_count: 1,
    }
}

fn payload_f32(payload: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap())
}

fn packed_affine_point(
    payload: &[u8],
    draw_index: usize,
    row0: usize,
    row1: usize,
    point: [f32; 2],
) -> [f32; 2] {
    let base = draw_index * SCENE_DRAW_UNIFORM_BYTES as usize;
    let apply_row = |row: usize| {
        payload_f32(payload, base + row * size_of::<f32>()) * point[0]
            + payload_f32(payload, base + (row + 1) * size_of::<f32>()) * point[1]
            + payload_f32(payload, base + (row + 2) * size_of::<f32>())
    };
    [apply_row(row0), apply_row(row1)]
}

fn assert_vec2_close(actual: [f32; 2], expected: [f32; 2]) {
    assert_close(actual[0], expected[0]);
    assert_close(actual[1], expected[1]);
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= 1.0e-5,
        "expected {expected}, got {actual}"
    );
}
