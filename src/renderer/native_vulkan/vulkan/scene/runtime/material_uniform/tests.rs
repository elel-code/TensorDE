use super::*;
use crate::engine::scene::{
    SceneBinaryDocument, SceneCullMode, SceneDepthTest, SceneMaterialConstantRecord,
    SceneMaterialHandle, SceneMaterialPassRecord, SceneMaterialRecord, SceneMaterialTextureRecord,
    ScenePipelineBlend, SceneRenderingDeviceDrawPrimitive, SceneResourceId, SceneResourceKind,
    SceneResourceRecord, SceneStringId, SceneTextureFormat,
};

#[test]
fn material_uniform_uses_default_when_draw_has_no_material() {
    let storage = SceneStorage::from_document(SceneBinaryDocument::default()).expect("storage");
    let draw = draw_with_material(SceneMaterialHandle(INVALID_MATERIAL_ID));

    let payload = pack_scene_material_uniforms(&storage, &[draw], 0.0);

    assert_eq!(
        payload.len(),
        SCENE_MATERIAL_UNIFORM_FLOATS * size_of::<f32>()
    );
    assert!(payload.iter().all(|byte| *byte == 0));
}

#[test]
fn material_uniform_packs_color_constant_into_first_vec4() {
    let storage = storage_with_constants("we/genericimage4", &[("tint", "[0.25,0.5,0.75,0.9]")]);
    let draw = draw_with_material(SceneMaterialHandle(0));

    let payload = pack_scene_material_uniforms(&storage, &[draw], 0.0);

    assert_eq!(f32_from_payload(&payload, 0), 0.25);
    assert_eq!(f32_from_payload(&payload, 4), 0.5);
    assert_eq!(f32_from_payload(&payload, 8), 0.75);
    assert_eq!(f32_from_payload(&payload, 12), 0.9);
}

#[test]
fn standard_material_multiplies_resolved_object_shadow_tint_and_alpha() {
    let storage = storage_with_constants("we/genericimage4", &[("tint", "[0.8,0.6,0.4,0.5]")]);
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.resolved_color = crate::engine::scene::SceneVec3 {
        x: 0.25,
        y: 0.5,
        z: 0.75,
    };
    draw.resolved_alpha = 0.3;

    let payload = pack_scene_material_uniforms(&storage, &[draw], 0.0);

    assert_eq!(f32_from_payload(&payload, 0), 0.2);
    assert_eq!(f32_from_payload(&payload, 4), 0.3);
    assert!((f32_from_payload(&payload, 8) - 0.3).abs() < f32::EPSILON);
    assert_eq!(f32_from_payload(&payload, 12), 0.15);
}

#[test]
fn synthetic_composite_uses_actual_pass_shader_uniform_layout() {
    let mut document = storage_with_constants("we/composelayer", &[])
        .document()
        .clone();
    document.strings.push("we/objectcomposite".to_owned());
    let composite_shader = SceneStringId((document.strings.len() - 1) as u32);
    let storage = SceneStorage::from_document(document).expect("composite storage");
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.shader_key = composite_shader;
    draw.primitive = SceneRenderingDeviceDrawPrimitive::FullscreenTriangle;
    draw.apply_resolved_visual = true;

    let payload = pack_scene_material_uniforms(&storage, &[draw], 0.0);

    for lane in 0..4 {
        assert_eq!(f32_from_payload(&payload, lane * 4), 1.0);
    }
}

#[test]
fn offscreen_object_source_defers_resolved_visual_to_object_composite() {
    let storage = storage_with_constants("we/genericimage4", &[("tint", "[0.8,0.6,0.4,0.5]")]);
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.resolved_color = crate::engine::scene::SceneVec3 {
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };
    draw.resolved_alpha = 0.3;
    draw.apply_resolved_visual = false;

    let payload = pack_scene_material_uniforms(&storage, &[draw], 0.0);

    assert_eq!(f32_from_payload(&payload, 0), 0.8);
    assert_eq!(f32_from_payload(&payload, 4), 0.6);
    assert_eq!(f32_from_payload(&payload, 8), 0.4);
    assert_eq!(f32_from_payload(&payload, 12), 0.5);
}

#[test]
fn object_composite_applies_only_resolved_visual_not_base_material_twice() {
    let storage = storage_with_constants("we/genericimage4", &[("tint", "[0.8,0.6,0.4,0.5]")]);
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.primitive = crate::engine::scene::SceneRenderingDeviceDrawPrimitive::FullscreenTriangle;
    draw.resolved_color = crate::engine::scene::SceneVec3 {
        x: 0.1,
        y: 0.2,
        z: 0.3,
    };
    draw.resolved_alpha = 0.3;

    let payload = pack_scene_material_uniforms(&storage, &[draw], 0.0);

    assert_eq!(f32_from_payload(&payload, 0), 0.1);
    assert_eq!(f32_from_payload(&payload, 4), 0.2);
    assert_eq!(f32_from_payload(&payload, 8), 0.3);
    assert_eq!(f32_from_payload(&payload, 12), 0.3);
}

#[test]
fn screen_group_composite_applies_base_and_resolved_visual_once() {
    let mut document = storage_with_constants("we/genericimage4", &[("tint", "[0.8,0.6,0.4,0.5]")])
        .document()
        .clone();
    document
        .strings
        .push("we/objectcomposite-screen-group".to_owned());
    let composite_shader = SceneStringId((document.strings.len() - 1) as u32);
    let storage = SceneStorage::from_document(document).expect("screen group composite storage");
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.shader_key = composite_shader;
    draw.primitive = SceneRenderingDeviceDrawPrimitive::FullscreenTriangle;
    draw.resolved_color = crate::engine::scene::SceneVec3 {
        x: 0.25,
        y: 0.5,
        z: 0.75,
    };
    draw.resolved_alpha = 0.3;

    let payload = pack_scene_material_uniforms(&storage, &[draw], 0.0);

    assert_eq!(f32_from_payload(&payload, 0), 0.2);
    assert_eq!(f32_from_payload(&payload, 4), 0.3);
    assert!((f32_from_payload(&payload, 8) - 0.3).abs() < f32::EPSILON);
    assert_eq!(f32_from_payload(&payload, 12), 0.15);
}

#[test]
fn waterwaves_uniform_uses_named_lanes_and_scene_time() {
    let storage = storage_with_constants(
        "effects/waterwaves__SLOTS_3__DUALWAVES_1",
        &[
            ("speed", "2.0"),
            ("scale", "5.0"),
            ("strength", "0.03"),
            ("direction", "-0.9"),
            ("speed2", "3.0"),
            ("scale2", "7.0"),
            ("direction2", "0.5"),
            ("offset2", "0.25"),
            ("exponent", "1.75"),
            ("exponent2", "2.25"),
        ],
    );
    let payload = pack_scene_material_uniforms(
        &storage,
        &[draw_with_material(SceneMaterialHandle(0))],
        4.25,
    );

    assert_eq!(f32_from_payload(&payload, 0), 4.25);
    assert_eq!(f32_from_payload(&payload, 4), 2.0);
    assert_eq!(f32_from_payload(&payload, 8), 5.0);
    assert_eq!(f32_from_payload(&payload, 12), 0.03);
    assert_eq!(f32_from_payload(&payload, 16), -0.9);
    assert_eq!(f32_from_payload(&payload, 20), 3.0);
    assert_eq!(f32_from_payload(&payload, 24), 7.0);
    assert_eq!(f32_from_payload(&payload, 28), 0.5);
    assert_eq!(f32_from_payload(&payload, 32), 0.25);
    assert_eq!(f32_from_payload(&payload, 36), 1.0);
    assert_eq!(f32_from_payload(&payload, 40), 1.75);
    assert_eq!(f32_from_payload(&payload, 44), 2.25);
}

#[test]
fn shimmer_uniform_packs_time_offset_then_effect_color_in_the_same_vec4() {
    let storage = storage_with_constants(
        "effects/shimmer__SLOTS_9",
        &[
            ("ui_editor_properties_timescale", "0.125"),
            ("ui_editor_properties_color", "[0.25,0.5,0.75]"),
            ("ui_editor_properties_brightness", "2.0"),
        ],
    );
    let payload =
        pack_scene_material_uniforms(&storage, &[draw_with_material(SceneMaterialHandle(0))], 3.0);

    assert_eq!(f32_from_payload(&payload, 8 * 4), 0.125);
    assert_eq!(f32_from_payload(&payload, 9 * 4), 0.25);
    assert_eq!(f32_from_payload(&payload, 10 * 4), 0.5);
    assert_eq!(f32_from_payload(&payload, 11 * 4), 0.75);
    assert_eq!(f32_from_payload(&payload, 6 * 4), 2.0);
}

#[test]
fn waterwaves_uv_field_batch_metadata_does_not_overwrite_stage_parameters() {
    let storage = storage_with_constants(
        "we/waterwaves-uv-field",
        &[
            ("waterwaves.stage_count", "1"),
            ("waterwaves.0.offset2", "0.125"),
            ("waterwaves.0.dualwaves", "1"),
            ("waterwaves.0.exponent", "1.75"),
            ("waterwaves.0.exponent2", "2.25"),
        ],
    );
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.effect_batch_atlas_tile = 6;
    draw.effect_batch_atlas_grid = [4, 3];

    let payload = pack_scene_material_uniforms(&storage, &[draw], 4.25);

    assert_eq!(f32_from_payload(&payload, 16), 21.25);
    assert!((f32_from_payload(&payload, 24) - 0.01).abs() < 1.0e-7);
    assert_eq!(f32_from_payload(&payload, 36), 1.0);
    assert_eq!(f32_from_payload(&payload, 40), 21.875);
    assert_eq!(f32_from_payload(&payload, 44), 200.0);
    assert_eq!(f32_from_payload(&payload, 48), 0.0);
    assert_eq!(f32_from_payload(&payload, 52), 1.0);
    assert_eq!(f32_from_payload(&payload, 56), 1.75);
    assert_eq!(f32_from_payload(&payload, 60), 2.25);
}

#[test]
fn waterwaves_visibility_mask_neutralizes_only_the_hidden_stage() {
    let storage = storage_with_constants(
        "we/waterwaves-uv-field",
        &[
            ("waterwaves.stage_count", "2"),
            ("waterwaves.0.strength", "0.5"),
            ("waterwaves.1.strength", "0.75"),
        ],
    );
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.effect_binding_start = 4;
    draw.effect_binding_count = 2;
    draw.effect_visibility_policy =
        crate::engine::scene::SceneRenderEffectVisibilityPolicy::WaterWavesStages;
    draw.resolved_effect_visibility_mask = 0b01;

    let payload = pack_scene_material_uniforms(&storage, &[draw], 0.0);

    assert_eq!(f32_from_payload(&payload, 6 * 4), 0.25);
    assert_eq!(f32_from_payload(&payload, 22 * 4), 0.0);
}

#[test]
fn direct_waterwaves_disabled_chain_has_zero_displacement() {
    let storage = storage_with_constants(
        "we/effect-waterwaves-direct__STAGES_3",
        &[
            ("waterwaves.stage_count", "3"),
            ("waterwaves.0.strength", "0.5"),
            ("waterwaves.1.strength", "0.75"),
            ("waterwaves.2.strength", "0.25"),
        ],
    );
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.effect_binding_start = 4;
    draw.effect_binding_count = 3;
    draw.effect_visibility_policy =
        crate::engine::scene::SceneRenderEffectVisibilityPolicy::WaterWavesStages;
    draw.resolved_effect_visibility_mask = 0;

    let payload = pack_scene_material_uniforms(&storage, &[draw], 0.0);

    for strength_lane in [10, 26, 42] {
        assert_eq!(f32_from_payload(&payload, strength_lane * 4), 0.0);
    }
}

#[test]
fn foliage_ripple_visibility_mask_neutralizes_each_owned_stage() {
    let storage = storage_with_constants(
        "we/image-foliage-ripple-composite",
        &[
            ("foliage.strength", "0.5"),
            ("ripple.strength", "0.75"),
        ],
    );
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.effect_binding_start = 8;
    draw.effect_binding_count = 2;
    draw.effect_visibility_policy =
        crate::engine::scene::SceneRenderEffectVisibilityPolicy::MaterialStages;
    draw.resolved_effect_visibility_mask = 0b01;

    let payload = pack_scene_material_uniforms(&storage, &[draw], 0.0);

    assert_eq!(f32_from_payload(&payload, 6 * 4), 0.5);
    assert_eq!(f32_from_payload(&payload, 21 * 4), 0.0);
}

#[test]
fn ripple_flow_visibility_neutralizes_both_typed_passes_independently() {
    let ripple_storage = storage_with_constants(
        "we/image-ripple-source",
        &[("ripplestrength", "0.6")],
    );
    let flow_storage = storage_with_constants(
        "we/image-ripple-flow-composite",
        &[("flow.strength", "2.5")],
    );
    let mut hidden = draw_with_material(SceneMaterialHandle(0));
    hidden.effect_binding_start = 5;
    hidden.effect_binding_count = 1;
    hidden.effect_visibility_policy =
        crate::engine::scene::SceneRenderEffectVisibilityPolicy::MaterialStages;
    hidden.resolved_effect_visibility_mask = 0;

    let ripple_payload = pack_scene_material_uniforms(&ripple_storage, &[hidden], 0.0);
    let flow_payload = pack_scene_material_uniforms(&flow_storage, &[hidden], 0.0);

    assert_eq!(f32_from_payload(&ripple_payload, 5 * 4), 0.0);
    assert_eq!(f32_from_payload(&flow_payload, 7 * 4), 0.0);
}

#[test]
fn waterwaves_composite_receives_its_atlas_tile_rectangle() {
    let storage = storage_with_constants("we/genericimage4", &[]);
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.effect_batch_atlas_tile = 6;
    draw.effect_batch_atlas_grid = [4, 3];

    let payload = pack_scene_material_uniforms(&storage, &[draw], 0.0);

    assert_eq!(f32_from_payload(&payload, 48), 0.25);
    assert_eq!(f32_from_payload(&payload, 52), 1.0 / 3.0);
    assert_eq!(f32_from_payload(&payload, 56), 0.5);
    assert_eq!(f32_from_payload(&payload, 60), 1.0 / 3.0);
}

#[test]
fn rounded_mask_uniform_packs_sdf_shape_parameters() {
    let storage = storage_with_constants(
        "effects/rounded_mask__SLOTS_1__B_SQUARE_0__C_ALPHA_ONLY_0__SOFT_1",
        &[
            ("Color", "\"0.2 0.4 0.6\""),
            ("Radius", "0.35"),
            ("Size", "\"0.8 0.9\""),
            ("Softness", "1.75"),
            ("ui_editor_properties_opacity", "0.7"),
        ],
    );
    let payload =
        pack_scene_material_uniforms(&storage, &[draw_with_material(SceneMaterialHandle(0))], 0.0);

    for (lane, expected) in [0.2, 0.4, 0.6, 0.35, 0.8, 0.9, 1.75, 0.7]
        .into_iter()
        .enumerate()
    {
        assert_eq!(f32_from_payload(&payload, lane * 4), expected);
    }
}

#[test]
fn rounded_mask_visibility_lane_disables_the_authored_sdf_stage() {
    let storage = storage_with_constants(
        "effects/rounded_mask__SLOTS_1__B_SQUARE_0__C_ALPHA_ONLY_0__SOFT_1",
        &[],
    );
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.effect_binding_start = 2;
    draw.effect_binding_count = 1;
    draw.effect_visibility_policy =
        crate::engine::scene::SceneRenderEffectVisibilityPolicy::FlatRoundedMask;
    draw.resolved_effect_visibility_mask = 0;

    let payload = pack_scene_material_uniforms(&storage, &[draw], 0.0);

    assert_eq!(f32_from_payload(&payload, 9 * 4), 0.0);
}

#[test]
fn final_scroll_visibility_neutralizes_motion_without_changing_repeat_data() {
    let storage = storage_with_constants(
        "we/image-scroll-final",
        &[
            ("scroll.speedx", "0.4"),
            ("scroll.speedy", "-0.25"),
            ("scroll.repeat", "\"2 3\""),
        ],
    );
    let mut draw = draw_with_material(SceneMaterialHandle(0));
    draw.effect_binding_start = 9;
    draw.effect_binding_count = 1;
    draw.effect_visibility_policy =
        crate::engine::scene::SceneRenderEffectVisibilityPolicy::MaterialStages;
    draw.resolved_effect_visibility_mask = 0;

    let payload = pack_scene_material_uniforms(&storage, &[draw], 3.0);

    assert_eq!(f32_from_payload(&payload, 5 * 4), 0.0);
    assert_eq!(f32_from_payload(&payload, 6 * 4), 0.0);
    assert_eq!(f32_from_payload(&payload, 7 * 4), 0.0);
    assert_eq!(f32_from_payload(&payload, 8 * 4), 2.0);
    assert_eq!(f32_from_payload(&payload, 9 * 4), 3.0);
}

#[test]
fn fused_eye_visibility_controls_iris_and_ripple_stages_independently() {
    let storage = storage_with_constants("we/puppet-iris-waterripple-final", &[]);
    let mut draw = draw_with_material_visibility(SceneMaterialHandle(0), 2, 0b01);

    let iris_only = pack_scene_material_uniforms(&storage, &[draw], 3.0);
    assert_eq!(f32_from_payload(&iris_only, 36 * 4), 1.0);
    assert_eq!(f32_from_payload(&iris_only, 37 * 4), 0.0);

    draw.resolved_effect_visibility_mask = 0b10;
    let ripple_only = pack_scene_material_uniforms(&storage, &[draw], 3.0);
    assert_eq!(f32_from_payload(&ripple_only, 36 * 4), 0.0);
    assert_eq!(f32_from_payload(&ripple_only, 37 * 4), 1.0);
}

#[test]
fn fused_colorkey_scroll_visibility_preserves_each_stage_identity() {
    let storage = storage_with_constants(
        "we/image-colorkey-scroll-final",
        &[
            ("colorkey.alpha", "0.25"),
            ("colorkey.flatten", "1"),
            ("scroll.speedx", "0.4"),
            ("scroll.repeat", "[2,3]"),
        ],
    );
    let mut draw = draw_with_material_visibility(SceneMaterialHandle(0), 2, 0b01);

    let colorkey_only = pack_scene_material_uniforms(&storage, &[draw], 3.0);
    assert_eq!(f32_from_payload(&colorkey_only, 7 * 4), 0.0);
    assert_eq!(f32_from_payload(&colorkey_only, 12 * 4), 0.25);
    assert_eq!(f32_from_payload(&colorkey_only, 19 * 4), 1.0);

    draw.resolved_effect_visibility_mask = 0b10;
    let scroll_only = pack_scene_material_uniforms(&storage, &[draw], 3.0);
    assert_eq!(f32_from_payload(&scroll_only, 7 * 4), 1.0);
    assert_eq!(f32_from_payload(&scroll_only, 12 * 4), 1.0);
    assert_eq!(f32_from_payload(&scroll_only, 19 * 4), 0.0);
}

#[test]
fn fused_rounded_opacity_visibility_falls_back_to_the_flat_base() {
    let storage = storage_with_constants(
        "we/flat-rounded-opacity-final",
        &[("opacity.alpha", "0.3")],
    );
    let mut draw = draw_with_material_visibility(SceneMaterialHandle(0), 2, 0b01);

    let rounded_only = pack_scene_material_uniforms(&storage, &[draw], 0.0);
    assert_eq!(f32_from_payload(&rounded_only, 9 * 4), 1.0);
    assert_eq!(f32_from_payload(&rounded_only, 10 * 4), 1.0);

    draw.resolved_effect_visibility_mask = 0b10;
    let opacity_only = pack_scene_material_uniforms(&storage, &[draw], 0.0);
    assert_eq!(f32_from_payload(&opacity_only, 9 * 4), 0.3);
    assert_eq!(f32_from_payload(&opacity_only, 10 * 4), 0.0);
}

#[test]
fn scroll_uniform_packs_time_signed_speed_and_repeat_inputs() {
    let storage = storage_with_constants(
        "effects/scroll__SLOTS_1",
        &[
            ("speedx", "-0.4"),
            ("speedy", "0.25"),
            ("repeat", "\"2 3\""),
        ],
    );
    let payload =
        pack_scene_material_uniforms(&storage, &[draw_with_material(SceneMaterialHandle(0))], 6.5);

    assert_eq!(f32_from_payload(&payload, 0), 6.5);
    assert_eq!(f32_from_payload(&payload, 4), -0.4);
    assert_eq!(f32_from_payload(&payload, 8), 0.25);
    assert_eq!(f32_from_payload(&payload, 16), 2.0);
    assert_eq!(f32_from_payload(&payload, 20), 3.0);
}

#[test]
fn skew_uniform_packs_authored_edge_offsets() {
    let storage = storage_with_constants(
        "effects/skew__SLOTS_1",
        &[
            ("top", "0.1"),
            ("bottom", "-0.39"),
            ("left", "0.2"),
            ("right", "-0.3"),
        ],
    );
    let payload =
        pack_scene_material_uniforms(&storage, &[draw_with_material(SceneMaterialHandle(0))], 0.0);

    for (lane, expected) in [0.1, -0.39, 0.2, -0.3].into_iter().enumerate() {
        assert_eq!(f32_from_payload(&payload, lane * 4), expected);
    }
}

#[test]
fn tech_circle_uniform_packs_bound_sector_value_and_time() {
    let storage = storage_with_constants(
        "effects/tech_circle__SLOTS_1__SECTOR_SEGMENTS_1",
        &[
            (
                "ui_editor_properties_1_color",
                "{\"value\":\"0.2 0.4 0.6\"}",
            ),
            ("ui_editor_properties_2_alpha", "0.8"),
            ("ui_editor_properties_3_speed", "0.1"),
            ("ui_editor_properties_4_ring_1_radius", "0.54"),
            ("ui_editor_properties_4_ring_1_width", "0.04"),
            (
                "ui_editor_properties_5_sector_1_width",
                "{\"script\":\"ignored\",\"value\":0.3}",
            ),
            ("ui_editor_properties_5_sector_segment_count", "5"),
            ("ui_editor_properties_5_sector_segment_width", "0.75"),
        ],
    );
    let payload =
        pack_scene_material_uniforms(&storage, &[draw_with_material(SceneMaterialHandle(0))], 4.5);

    assert_eq!(f32_from_payload(&payload, 0), 0.2);
    assert_eq!(f32_from_payload(&payload, 4), 0.4);
    assert_eq!(f32_from_payload(&payload, 8), 0.6);
    assert_eq!(f32_from_payload(&payload, 12), 0.8);
    assert_eq!(f32_from_payload(&payload, 16), 4.5);
    assert_eq!(f32_from_payload(&payload, 20), 0.1);
    assert_eq!(f32_from_payload(&payload, 28), 0.54);
    assert_eq!(f32_from_payload(&payload, 32), 0.04);
    assert_eq!(f32_from_payload(&payload, 48), 0.3);
    assert_eq!(f32_from_payload(&payload, 52), 5.0);
    assert_eq!(f32_from_payload(&payload, 56), 0.75);
}

#[test]
fn audio_bars_uniform_packs_zero_spectrum_baseline_shape() {
    let storage = storage_with_constants(
        "effects/simple_audio_bars__SLOTS_1__SHAPE_7",
        &[
            ("Bar Color", "{\"value\":\"0.2 0.4 0.6\"}"),
            ("ui_editor_properties_opacity", "0.8"),
            ("Bar Count", "12"),
            ("Bar Spacing", "0.31"),
            ("Lower/Upper Bar Bounds", "\"0.1 0.1\""),
            ("Minimum Height (Will be multiplied by the bar width) ", "1"),
            ("Radius", "1"),
            ("Volume Factor", "0.5"),
            ("Anti-alias blurring ", "\"0.01 0.04\""),
        ],
    );
    let payload =
        pack_scene_material_uniforms(&storage, &[draw_with_material(SceneMaterialHandle(0))], 0.0);

    for (lane, expected) in [
        0.2, 0.4, 0.6, 0.8, 12.0, 0.31, 0.1, 0.1, 1.0, 1.0, 0.5, 0.01, 0.04,
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(f32_from_payload(&payload, lane * 4), expected);
    }
    assert_eq!(f32_from_payload(&payload, 16 * 4), 0.0);
    assert_eq!(f32_from_payload(&payload, 48 * 4), 0.0);
}

#[test]
fn audio_bars_uniform_duplicates_mono_spectrum_into_stereo_vec4_arrays() {
    let storage = storage_with_constants(
        "effects/simple_audio_bars__SLOTS_1__SHAPE_7",
        &[],
    );
    let spectrum = std::array::from_fn(|band| band as f32 / 31.0);
    let payload = pack_scene_material_uniforms_with_spectrum(
        &storage,
        &[draw_with_material(SceneMaterialHandle(0))],
        0.0,
        Some(&spectrum),
    );

    assert_eq!(payload.len(), SCENE_MATERIAL_UNIFORM_BYTES as usize);
    for band in 0..32 {
        assert_eq!(f32_from_payload(&payload, (16 + band) * 4), spectrum[band]);
        assert_eq!(f32_from_payload(&payload, (48 + band) * 4), spectrum[band]);
    }
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
    };

    let values = final_audio_bars_values(&parameters, &storage, &draw, None);

    assert_eq!(values[29], 1.0);
    assert_eq!(values[30], 1000.0);
}

#[test]
fn fused_final_audio_program_requests_the_system_spectrum_adapter() {
    let final_storage = storage_with_constants("we/audio-bars-final", &[]);
    let legacy_storage = storage_with_constants(
        "effects/simple_audio_bars__SLOTS_1__SHAPE_7",
        &[],
    );
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
    let payload = pack_scene_material_uniforms(
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
    let payload =
        pack_scene_material_uniforms(&storage, &[draw_with_material(SceneMaterialHandle(0))], 0.0);

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
    let payload =
        pack_scene_material_uniforms(&storage, &[draw_with_material(SceneMaterialHandle(0))], 3.5);

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
    let payload =
        pack_scene_material_uniforms(&storage, &[draw_with_material(SceneMaterialHandle(0))], 0.0);

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
    let payload =
        pack_scene_material_uniforms(&storage, &[draw_with_material(SceneMaterialHandle(0))], 3.5);

    let expected = [
        3.5, 0.125, 1.5, 0.375, 2.0, 0.25, 0.75, 0.625, 0.875, 0.5, 0.2, -0.25,
        0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.11, 0.22, 0.33, 0.44, -0.2,
        -0.3, -0.4, -0.5,
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
    let payload =
        pack_scene_material_uniforms(&storage, &[draw_with_material(SceneMaterialHandle(0))], 8.5);

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
    let payload =
        pack_scene_material_uniforms(&storage, &[draw_with_material(SceneMaterialHandle(0))], 0.0);

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
    let payload =
        pack_scene_material_uniforms(&storage, &[draw_with_material(SceneMaterialHandle(0))], 0.0);

    let expected = [0.75, -2.0, 3.25, 0.0, 0.2, 0.4, 0.8, 1.0];
    for (lane, expected) in expected.into_iter().enumerate() {
        assert_eq!(f32_from_payload(&payload, lane * 4), expected);
    }
}

fn storage_with_constants(shader: &str, constants: &[(&str, &str)]) -> SceneStorage {
    let mut strings = vec![shader.to_owned()];
    let mut material_constants = Vec::with_capacity(constants.len());
    for (name, value) in constants {
        let name_id = SceneStringId(strings.len() as u32);
        strings.push((*name).to_owned());
        let value_id = SceneStringId(strings.len() as u32);
        strings.push((*value).to_owned());
        material_constants.push(SceneMaterialConstantRecord {
            name: name_id,
            value_json: value_id,
        });
    }
    SceneStorage::from_document(SceneBinaryDocument {
        strings,
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
            constant_count: material_constants.len() as u32,
            pipeline_blend: ScenePipelineBlend::Normal,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            alpha_writing: SceneStringId::NONE,
            clear_target: false,
        }],
        material_constants,
        ..SceneBinaryDocument::default()
    })
    .expect("storage")
}

fn storage_with_padded_mask(
    shader: &str,
    storage_width: u32,
    storage_height: u32,
    width: u32,
    height: u32,
) -> SceneStorage {
    let storage = storage_with_constants(shader, &[]);
    let mut document = storage.document().clone();
    let resource = SceneResourceId(7);
    document.resources.push(SceneResourceRecord {
        id: resource,
        kind: SceneResourceKind::TextureTex,
        path: SceneStringId::NONE,
        source: SceneStringId::NONE,
        payload_offset: 0,
        payload_len: 0,
    });
    document.textures.push(SceneTextureRecord {
        resource,
        format: SceneTextureFormat::Bc4UnormBlock,
        source_runtime_format: 9,
        payload_format: 0,
        sampler_flags: 0,
        width,
        height,
        storage_width,
        storage_height,
        mip_start: 0,
        mip_count: 0,
        texv_tag: SceneStringId::NONE,
        texb_tag: SceneStringId::NONE,
        payload_offset: 0,
        payload_len: 0,
        alpha_coverage_rows: [u32::MAX;
            crate::engine::scene::SCENE_TEXTURE_ALPHA_COVERAGE_GRID_SIZE],
    });
    document.material_textures.push(SceneMaterialTextureRecord {
        slot: 1,
        resource,
        path: SceneStringId::NONE,
    });
    document.material_passes[0].texture_count = 1;
    SceneStorage::from_document(document).expect("storage with padded mask")
}

fn draw_with_material(material: SceneMaterialHandle) -> SceneRenderingDeviceMeshDraw {
    SceneRenderingDeviceMeshDraw {
        primitive: SceneRenderingDeviceDrawPrimitive::ObjectMesh,
        shader_key: SceneStringId::NONE,
        mesh_index: 0,
        resolved_object_index: 0,
        clip_transform: [[0.0; 4]; 4],
        authored_source_extent: [0.0; 2],
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
        effect_visibility_policy:
            crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
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

fn draw_with_material_visibility(
    material: SceneMaterialHandle,
    binding_count: u32,
    visibility_mask: u32,
) -> SceneRenderingDeviceMeshDraw {
    let mut draw = draw_with_material(material);
    draw.effect_binding_start = 0;
    draw.effect_binding_count = binding_count;
    draw.effect_visibility_policy =
        crate::engine::scene::SceneRenderEffectVisibilityPolicy::MaterialStages;
    draw.resolved_effect_visibility_mask = visibility_mask;
    draw
}

fn f32_from_payload(payload: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap())
}
