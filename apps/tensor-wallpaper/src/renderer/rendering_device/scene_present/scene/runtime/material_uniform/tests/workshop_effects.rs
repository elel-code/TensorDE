use super::*;

#[test]
fn pulse_uniform_keeps_author_parameters_and_stereo_16_band_max_pooling_typed() {
    let storage = storage_with_constants(
        "effects/pulse__SLOTS_3__AUDIOPROCESSING_3__BLENDMODE_2__PULSEALPHA_1__PULSECOLOR_0",
        &[
            ("amount", "0.7"),
            ("audioamount", "0.5"),
            ("audiobounds", r#""0.25 0.75""#),
            ("audioexponent", "2.0"),
            ("bounds", r#""0.1 0.8""#),
            ("frequencymax", "7"),
            ("frequencymin", "2"),
            ("phase", "1.25"),
            ("speed", "2.5"),
            ("tinthigh", r#""0.5 0.6 0.7""#),
            ("tintlow", r#""0.2 0.3 0.4""#),
        ],
    );
    let mut left = [0.0; 64];
    let mut right = [0.0; 64];
    left[..4].copy_from_slice(&[0.1, 0.4, 0.2, 0.8]);
    right[..4].copy_from_slice(&[0.9, 0.3, 0.6, 0.2]);
    let spectrum = crate::engine::scene::StereoSpectrum64 { left, right };
    let draw = draw_with_material(SceneMaterialHandle(0));
    let payload = super::pack_scene_material_uniforms_with_frame_inputs(
        &storage,
        &[draw],
        3.5,
        TEST_OUTPUT_EXTENT,
        SceneMaterialFrameInputs {
            average_spectrum32: None,
            stereo_spectrum64: Some(&spectrum),
            parallax_position: [0.5; 2],
            audio_material_values: &[],
            material_scalar_values: &[],
        },
    );

    assert_eq!(f32_from_payload(&payload, 0), 1.0);
    assert_eq!(f32_from_payload(&payload, 4), 0.0);
    assert_eq!(f32_from_payload(&payload, 16), 3.5);
    assert_eq!(f32_from_payload(&payload, 20), 2.5);
    assert_eq!(f32_from_payload(&payload, 24), 1.25);
    assert_eq!(f32_from_payload(&payload, 28), 0.7);
    assert_eq!(f32_from_payload(&payload, 32), 0.1);
    assert_eq!(f32_from_payload(&payload, 36), 0.8);
    assert_eq!(f32_from_payload(&payload, 52), 2.0);
    assert_eq!(f32_from_payload(&payload, 56), 7.0);
    assert_eq!(f32_from_payload(&payload, 60), 2.0);
    assert_eq!(f32_from_payload(&payload, 64), 0.25);
    assert_eq!(f32_from_payload(&payload, 68), 0.75);
    assert_eq!(f32_from_payload(&payload, 72), 0.5);
    assert_eq!(f32_from_payload(&payload, 80), 0.2);
    assert_eq!(f32_from_payload(&payload, 96), 0.5);
    assert_eq!(f32_from_payload(&payload, 128), 0.8);
    assert_eq!(f32_from_payload(&payload, 192), 0.9);
}

#[test]
fn depth_parallax_uniform_keeps_pointer_and_authored_controls_typed() {
    let storage = storage_with_constants(
        "effects/depthparallax__SLOTS_3__QUALITY_2",
        &[("center", "1"), ("scale", r#""-0.3 0""#), ("sens", "0.05")],
    );
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.primitive = crate::engine::scene::SceneRenderingDeviceDrawPrimitive::FullscreenTriangle;
    let payload = super::pack_scene_material_uniforms_with_frame_inputs(
        &storage,
        &[draw],
        0.0,
        TEST_OUTPUT_EXTENT,
        SceneMaterialFrameInputs {
            average_spectrum32: None,
            stereo_spectrum64: None,
            parallax_position: [0.2, 0.8],
            audio_material_values: &[],
            material_scalar_values: &[],
        },
    );

    assert_eq!(f32_from_payload(&payload, 32), 0.2);
    assert_eq!(f32_from_payload(&payload, 36), 0.8);
    assert_eq!(f32_from_payload(&payload, 40), -0.3);
    assert_eq!(f32_from_payload(&payload, 44), 0.0);
    assert_eq!(f32_from_payload(&payload, 48), 0.05);
    assert_eq!(f32_from_payload(&payload, 52), 1.0);
}

#[test]
fn shake_uniform_keeps_authored_bounds_friction_and_motion_controls() {
    let storage = storage_with_constants(
        "effects/shake__SLOTS_7__DIRECTION_1",
        &[
            ("bounds", r#""0.9 1""#),
            ("friction", r#""1 1""#),
            ("speed", "0.5"),
            ("strength", "0.11"),
        ],
    );
    let payload = pack_test_scene_material_uniforms(
        &storage,
        &[draw_with_material(SceneMaterialHandle(0))],
        2.0,
    );

    assert_eq!(f32_from_payload(&payload, 0), 2.0);
    assert_eq!(f32_from_payload(&payload, 4), 0.5);
    assert_eq!(f32_from_payload(&payload, 8), 0.11);
    assert_eq!(f32_from_payload(&payload, 16), 0.9);
    assert_eq!(f32_from_payload(&payload, 20), 1.0);
    assert_eq!(f32_from_payload(&payload, 24), 1.0);
    assert_eq!(f32_from_payload(&payload, 28), 1.0);
}

#[test]
fn masked_tint_uniform_preserves_storage_and_logical_mask_extent() {
    let storage = storage_with_padded_mask("effects/tint__SLOTS_3", 128, 64, 100, 50);
    let payload = pack_test_scene_material_uniforms(
        &storage,
        &[draw_with_material(SceneMaterialHandle(0))],
        0.0,
    );

    assert_eq!(f32_from_payload(&payload, 16), 128.0);
    assert_eq!(f32_from_payload(&payload, 20), 64.0);
    assert_eq!(f32_from_payload(&payload, 24), 100.0);
    assert_eq!(f32_from_payload(&payload, 28), 50.0);
}

#[test]
fn waterflow_uniform_packs_motion_and_logical_flow_extent() {
    let storage = storage_with_padded_mask("effects/waterflow__SLOTS_7", 128, 64, 100, 50);
    let mut document = storage.document().clone();
    document.strings.extend([
        "speed".to_owned(),
        "0.03".to_owned(),
        "feather".to_owned(),
        "0.5".to_owned(),
        "strength".to_owned(),
        "2.6".to_owned(),
        "phasescale".to_owned(),
        "2.99".to_owned(),
    ]);
    document.material_passes[0].constant_start = 0;
    document.material_passes[0].constant_count = 4;
    document.material_constants = (0..4)
        .map(|index| SceneMaterialConstantRecord {
            name: SceneStringId(1 + index * 2),
            value_json: SceneStringId(2 + index * 2),
        })
        .collect();
    let storage = SceneStorage::from_document(document).expect("waterflow storage");
    let payload = pack_test_scene_material_uniforms(
        &storage,
        &[draw_with_material(SceneMaterialHandle(0))],
        4.25,
    );

    for (lane, expected) in [4.25, 0.03, 0.5, 2.6, 2.99].into_iter().enumerate() {
        assert_eq!(f32_from_payload(&payload, lane * 4), expected);
    }
    assert_eq!(f32_from_payload(&payload, 32), 128.0);
    assert_eq!(f32_from_payload(&payload, 36), 64.0);
    assert_eq!(f32_from_payload(&payload, 40), 100.0);
    assert_eq!(f32_from_payload(&payload, 44), 50.0);
}

#[test]
fn waterwaves_uniform_uses_storage_and_logical_mask_extents() {
    let storage = storage_with_padded_mask("effects/waterwaves__SLOTS_7", 128, 64, 100, 50);
    let payload = pack_test_scene_material_uniforms(
        &storage,
        &[draw_with_material(SceneMaterialHandle(0))],
        0.0,
    );

    assert_eq!(f32_from_payload(&payload, 48), 128.0);
    assert_eq!(f32_from_payload(&payload, 52), 64.0);
    assert_eq!(f32_from_payload(&payload, 56), 100.0);
    assert_eq!(f32_from_payload(&payload, 60), 50.0);
}

#[test]
fn foliage_sway_uniform_uses_authored_uv_motion_parameters() {
    let storage = storage_with_constants(
        "effects/foliagesway__SLOTS_1",
        &[
            ("speeduv", "5.0"),
            ("strength", "0.5"),
            ("phase", "2.0"),
            ("power", "2.0"),
            ("scale", "0.05"),
            ("ratio", "2.11"),
            ("scrolldirection", "0.25"),
        ],
    );
    let payload = pack_test_scene_material_uniforms(
        &storage,
        &[draw_with_material(SceneMaterialHandle(0))],
        3.5,
    );

    assert_eq!(f32_from_payload(&payload, 0), 3.5);
    assert_eq!(f32_from_payload(&payload, 4), 5.0);
    assert_eq!(f32_from_payload(&payload, 8), 0.5);
    assert_eq!(f32_from_payload(&payload, 12), 2.0);
    assert_eq!(f32_from_payload(&payload, 16), 2.0);
    assert_eq!(f32_from_payload(&payload, 20), 0.05);
    assert_eq!(f32_from_payload(&payload, 24), 2.11);
    assert_eq!(f32_from_payload(&payload, 28), 0.25);
}

#[test]
fn opacity_uniform_maps_instance_alpha() {
    let storage = storage_with_constants("effects/opacity__SLOTS_1", &[("alpha", "0.97")]);
    let payload = pack_test_scene_material_uniforms(
        &storage,
        &[draw_with_material(SceneMaterialHandle(0))],
        0.0,
    );

    assert_eq!(f32_from_payload(&payload, 0), 0.97);
}

#[test]
fn blur_gaussian_uniform_maps_authored_scale() {
    let storage = storage_with_constants(
        "effects/blur_gaussian__SLOTS_1__VERTICAL_1",
        &[("scale", "[2.25,3.5]")],
    );
    let payload = pack_test_scene_material_uniforms(
        &storage,
        &[draw_with_material(SceneMaterialHandle(0))],
        0.0,
    );

    assert_eq!(f32_from_payload(&payload, 0), 2.25);
    assert_eq!(f32_from_payload(&payload, 4), 3.5);
}

#[test]
fn blur_combine_uniform_matches_composite_alpha_offset_and_color_contract() {
    let storage = storage_with_constants(
        "effects/blur_combine__SLOTS_5__BLENDMODE_1__COMPOSITE_1",
        &[
            ("compositealpha", "0.75"),
            ("compositeoffset", "[-2.0,3.25]"),
            ("compositecolor", "[0.2,0.4,0.8]"),
        ],
    );
    let payload = pack_test_scene_material_uniforms(
        &storage,
        &[draw_with_material(SceneMaterialHandle(0))],
        0.0,
    );

    let expected = [0.75, -2.0, 3.25, 0.0, 0.2, 0.4, 0.8, 1.0];
    for (lane, expected) in expected.into_iter().enumerate() {
        assert_eq!(f32_from_payload(&payload, lane * 4), expected);
    }
}
