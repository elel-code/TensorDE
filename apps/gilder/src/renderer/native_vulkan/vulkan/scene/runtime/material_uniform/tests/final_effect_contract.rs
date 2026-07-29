use super::*;

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

    let payload = pack_test_scene_material_uniforms(&storage, &[draw], 3.0);

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

    let iris_only = pack_test_scene_material_uniforms(&storage, &[draw], 3.0);
    assert_eq!(f32_from_payload(&iris_only, 36 * 4), 1.0);
    assert_eq!(f32_from_payload(&iris_only, 37 * 4), 0.0);

    draw.resolved_effect_visibility_mask = 0b10;
    let ripple_only = pack_test_scene_material_uniforms(&storage, &[draw], 3.0);
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

    let colorkey_only = pack_test_scene_material_uniforms(&storage, &[draw], 3.0);
    assert_eq!(f32_from_payload(&colorkey_only, 7 * 4), 0.0);
    assert_eq!(f32_from_payload(&colorkey_only, 12 * 4), 0.25);
    assert_eq!(f32_from_payload(&colorkey_only, 19 * 4), 1.0);

    draw.resolved_effect_visibility_mask = 0b10;
    let scroll_only = pack_test_scene_material_uniforms(&storage, &[draw], 3.0);
    assert_eq!(f32_from_payload(&scroll_only, 7 * 4), 1.0);
    assert_eq!(f32_from_payload(&scroll_only, 12 * 4), 1.0);
    assert_eq!(f32_from_payload(&scroll_only, 19 * 4), 0.0);
}

#[test]
fn fused_rounded_opacity_visibility_falls_back_to_the_flat_base() {
    let storage =
        storage_with_constants("we/flat-rounded-opacity-final", &[("opacity.alpha", "0.3")]);
    let mut draw = draw_with_material_visibility(SceneMaterialHandle(0), 2, 0b01);

    let rounded_only = pack_test_scene_material_uniforms(&storage, &[draw], 0.0);
    assert_eq!(f32_from_payload(&rounded_only, 9 * 4), 1.0);
    assert_eq!(f32_from_payload(&rounded_only, 10 * 4), 1.0);

    draw.resolved_effect_visibility_mask = 0b10;
    let opacity_only = pack_test_scene_material_uniforms(&storage, &[draw], 0.0);
    assert_eq!(f32_from_payload(&opacity_only, 9 * 4), 0.3);
    assert_eq!(f32_from_payload(&opacity_only, 10 * 4), 0.0);
}

#[test]
fn framebuffer_water_opacity_uniform_preserves_two_stage_identity_and_parameters() {
    let storage = storage_with_constants(
        "we/framebuffer-water-quantized-water-opacity",
        &[
            ("waves.speed", "2.0"),
            ("waves.scale", "5.0"),
            ("waves.strength", "0.25"),
            ("waves.direction", "0.4"),
            ("waves.exponent", "1.5"),
            ("opacity.alpha", "0.7"),
        ],
    );
    let mut draw = draw_with_material_visibility(SceneMaterialHandle(0), 2, 0b10);
    draw.resolved_color = crate::engine::scene::SceneVec3 {
        x: 0.1,
        y: 0.2,
        z: 0.3,
    };
    draw.resolved_alpha = 0.125;

    let payload = pack_test_scene_material_uniforms(&storage, &[draw], 7.0);

    for (lane, expected) in [
        (0, 7.0),
        (1, 2.0),
        (2, 5.0),
        (3, 0.25),
        (4, 0.4),
        (5, 1.5),
        (6, 0.7),
        (8, 0.0),
        (9, 1.0),
    ] {
        assert_eq!(f32_from_payload(&payload, lane * 4), expected);
    }

    for visibility_mask in 0_u32..4 {
        draw.resolved_effect_visibility_mask = visibility_mask;
        let payload = pack_test_scene_material_uniforms(&storage, &[draw], 7.0);
        for stage in 0..2 {
            assert_eq!(
                f32_from_payload(&payload, (8 + stage) * 4),
                f32::from((visibility_mask & (1 << stage) != 0) as u8),
                "visibility mask {visibility_mask:#04b} changed intermediate stage {stage}"
            );
        }
    }
}

#[test]
fn framebuffer_water_shake_uniform_preserves_flow_and_visibility() {
    let storage = storage_with_padded_mask(
        "we/framebuffer-water-quantized-shake-final",
        128,
        64,
        100,
        50,
    );
    let mut document = storage.document().clone();
    for (name_text, value) in [
        ("shake.speed", "3.0"),
        ("shake.strength", "0.2"),
        ("shake.bounds", "[0.1,0.9]"),
        ("shake.friction", "[2.0,4.0]"),
    ] {
        let name = SceneStringId(document.strings.len() as u32);
        document.strings.push(name_text.to_owned());
        let value_id = SceneStringId(document.strings.len() as u32);
        document.strings.push(value.to_owned());
        document
            .material_constants
            .push(SceneMaterialConstantRecord {
                name,
                value_json: value_id,
            });
    }
    document.material_passes[0].constant_count = document.material_constants.len() as u32;
    let storage = SceneStorage::from_document(document).expect("framebuffer water storage");
    let mut draw = draw_with_material_visibility(SceneMaterialHandle(0), 1, 1);
    draw.resolved_color = crate::engine::scene::SceneVec3 {
        x: 0.1,
        y: 0.2,
        z: 0.3,
    };
    draw.resolved_alpha = 0.125;

    let payload = pack_test_scene_material_uniforms(&storage, &[draw], 7.0);

    for (lane, expected) in [
        (0, 7.0),
        (1, 3.0),
        (2, 0.2),
        (4, 0.1),
        (5, 0.9),
        (6, 2.0),
        (7, 4.0),
        (8, 128.0),
        (9, 64.0),
        (10, 100.0),
        (11, 50.0),
        (12, 1.0),
    ] {
        assert_eq!(f32_from_payload(&payload, lane * 4), expected);
    }

    for visibility_mask in 0_u32..2 {
        draw.resolved_effect_visibility_mask = visibility_mask;
        let payload = pack_test_scene_material_uniforms(&storage, &[draw], 7.0);
        assert_eq!(
            f32_from_payload(&payload, 12 * 4),
            f32::from((visibility_mask & 1 != 0) as u8),
            "visibility mask {visibility_mask:#03b} changed shake identity"
        );
    }
}

#[test]
fn framebuffer_water_stage_partition_preserves_all_sixteen_visibility_masks() {
    let intermediate_storage =
        storage_with_constants("we/framebuffer-water-quantized-water-opacity", &[]);
    let final_storage =
        storage_with_padded_mask("we/framebuffer-water-quantized-shake-final", 1, 1, 1, 1);
    let mut intermediate = draw_with_material_visibility(SceneMaterialHandle(0), 2, 0);
    let mut final_draw = draw_with_material_visibility(SceneMaterialHandle(0), 1, 0);

    for authored_mask in 0_u32..16 {
        intermediate.resolved_effect_visibility_mask = (authored_mask >> 1) & 0b11;
        final_draw.resolved_effect_visibility_mask = (authored_mask >> 3) & 1;
        let intermediate_payload =
            pack_test_scene_material_uniforms(&intermediate_storage, &[intermediate], 0.0);
        let final_payload = pack_test_scene_material_uniforms(&final_storage, &[final_draw], 0.0);

        assert_eq!(
            f32_from_payload(&intermediate_payload, 8 * 4),
            f32::from((authored_mask & 0b0010 != 0) as u8),
            "authored visibility mask {authored_mask:#06b} changed water ownership"
        );
        assert_eq!(
            f32_from_payload(&intermediate_payload, 9 * 4),
            f32::from((authored_mask & 0b0100 != 0) as u8),
            "authored visibility mask {authored_mask:#06b} changed opacity ownership"
        );
        assert_eq!(
            f32_from_payload(&final_payload, 12 * 4),
            f32::from((authored_mask & 0b1000 != 0) as u8),
            "authored visibility mask {authored_mask:#06b} changed shake ownership"
        );
    }
}
