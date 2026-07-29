use super::*;

#[test]
fn oscilloscope_uniform_carries_the_authored_local_target_resolution() {
    let storage = storage_with_constants(
        "effects/audio_responsive_oscilloscope__SLOTS_5__RESOLUTION_16",
        &[],
    );
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.authored_source_extent = [905.0, 200.0];

    let payload = pack_test_scene_material_uniforms(&storage, &[draw], 0.0);

    assert_eq!(f32_from_payload(&payload, 32 * 4), 905.0);
    assert_eq!(f32_from_payload(&payload, 33 * 4), 200.0);
    assert_eq!(f32_from_payload(&payload, 34 * 4), 905.0);
    assert_eq!(f32_from_payload(&payload, 35 * 4), 200.0);
}
#[test]
fn final_audio_bars_uses_object_local_source_resolution_for_deformity() {
    let storage = storage_with_constants("we/audio-bars-final", &[]);
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.authored_source_extent = [1000.0, 1000.0];
    let pass = first_material_pass(&storage, SceneMaterialHandle(0)).expect("material pass");
    let parameters = MaterialParameters {
        storage: &storage,
        pass,
        scalar_overrides: &[],
    };

    let values = final_audio_bars_values(&parameters, &storage, &draw, None);

    assert_eq!(values[29], 1.0);
    assert_eq!(values[30], 1000.0);
}

#[test]
fn fused_final_audio_program_requests_the_system_spectrum_adapter() {
    let final_storage = storage_with_constants("we/audio-bars-final", &[]);
    let legacy_storage = storage_with_constants("effects/simple_audio_bars__SLOTS_1__SHAPE_7", &[]);
    let unrelated_storage = storage_with_constants("we/image-scroll-final", &[]);

    assert!(material_uses_audio_spectrum(
        &final_storage,
        SceneMaterialHandle(0)
    ));
    assert!(material_uses_audio_spectrum(
        &legacy_storage,
        SceneMaterialHandle(0)
    ));
    assert!(!material_uses_audio_spectrum(
        &unrelated_storage,
        SceneMaterialHandle(0)
    ));
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
    let storage = storage_with_padded_mask("effects/waterwaves__SLOTS_3", 128, 64, 100, 50);
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
fn auto_sway_uniform_matches_the_typed_shader_lane_contract() {
    let storage = storage_with_constants(
        "effects/auto_sway__SLOTS_1__DEBUG_0__DEBUG_NO_ALPHA_1__NODE_COUNT_4",
        &[
            ("timeoffset", "0.125"),
            ("speed", "1.5"),
            ("inertia", "0.375"),
            ("sigment", "2.0"),
            ("weightCenterOffset", "0.25"),
            ("smoothDistance", "0.75"),
            ("directionalCompensation", "0.625"),
            ("strength", "0.875"),
            ("末端阻尼", "0.5"),
            ("xFeather", "0.2"),
            ("windDirectionOffset", "-0.25"),
            ("center1", "[0.1,0.2]"),
            ("center2", "[0.3,0.4]"),
            ("center3", "[0.5,0.6]"),
            ("center4", "[0.7,0.8]"),
            ("size1", "0.11"),
            ("size2", "0.22"),
            ("size3", "0.33"),
            ("size4", "0.44"),
            ("angle2", "-0.2"),
            ("angle3", "-0.3"),
            ("angle4", "-0.4"),
            ("angle5", "-0.5"),
        ],
    );
    let payload = pack_test_scene_material_uniforms(
        &storage,
        &[draw_with_material(SceneMaterialHandle(0))],
        3.5,
    );

    let expected = [
        3.5, 0.125, 1.5, 0.375, 2.0, 0.25, 0.75, 0.625, 0.875, 0.5, 0.2, -0.25, 0.1, 0.2, 0.3, 0.4,
        0.5, 0.6, 0.7, 0.8, 0.11, 0.22, 0.33, 0.44, -0.2, -0.3, -0.4, -0.5,
    ];
    for (lane, expected) in expected.into_iter().enumerate() {
        assert_eq!(f32_from_payload(&payload, lane * 4), expected);
    }
}

#[test]
fn procedural_noise_uniform_matches_offset_scale_magnitude_and_step_time_contract() {
    let storage = storage_with_constants(
        "effects/procedural_noise__SLOTS_1__AA_CATEGORY_1__BLENDMODE_20__STEPANIM_1",
        &[
            ("animationspeed", "2.5"),
            ("scrollirection", "-0.75"),
            ("scrollspeed", "1.25"),
            ("Offset", "[0.1,0.2]"),
            ("Scale", "[3.0,4.0]"),
            ("Magnitude", "[5.0,6.0]"),
            ("Seed", "7.0"),
            ("FPS", "24.0"),
            ("Opacity", "0.625"),
        ],
    );
    let payload = pack_test_scene_material_uniforms(
        &storage,
        &[draw_with_material(SceneMaterialHandle(0))],
        8.5,
    );

    let expected = [
        8.5, 2.5, -0.75, 1.25, 0.1, 0.2, 3.0, 4.0, 5.0, 6.0, 7.0, 24.0, 0.625,
    ];
    for (lane, expected) in expected.into_iter().enumerate() {
        assert_eq!(f32_from_payload(&payload, lane * 4), expected);
    }
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
