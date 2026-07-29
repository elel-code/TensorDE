use super::*;

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
fn fused_engine_audio_program_requests_the_system_spectrum_adapter() {
    let final_storage = storage_with_constants("we/audio-bars-final", &[]);
    let unrelated_storage = storage_with_constants("we/image-scroll-final", &[]);

    assert!(material_uses_audio_spectrum(
        &final_storage,
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
