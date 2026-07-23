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
fn framebuffer_water_uniform_preserves_stage_identity_extents_and_authored_parameters() {
    let storage = storage_with_padded_mask(
        "we/framebuffer-water-quantized-final",
        128,
        64,
        100,
        50,
    );
    let mut document = storage.document().clone();
    document.project.logical_width = 1920;
    document.project.logical_height = 1080;
    for (name_text, value) in [
        ("waves.speed", "2.0"),
        ("waves.scale", "5.0"),
        ("waves.strength", "0.25"),
        ("waves.direction", "0.4"),
        ("waves.exponent", "1.5"),
        ("opacity.alpha", "0.7"),
        ("shake.speed", "3.0"),
        ("shake.strength", "0.2"),
        ("shake.bounds", "[0.1,0.9]"),
        ("shake.friction", "[2.0,4.0]"),
    ] {
        let name = SceneStringId(document.strings.len() as u32);
        document.strings.push(name_text.to_owned());
        let value_id = SceneStringId(document.strings.len() as u32);
        document.strings.push(value.to_owned());
        document.material_constants.push(SceneMaterialConstantRecord {
            name,
            value_json: value_id,
        });
    }
    document.material_passes[0].constant_count = document.material_constants.len() as u32;
    let storage = SceneStorage::from_document(document).expect("framebuffer water storage");
    let mut draw = draw_with_material_visibility(SceneMaterialHandle(0), 4, 0b1010);
    draw.authored_source_extent = [640.0, 360.0];
    draw.resolved_color = crate::engine::scene::SceneVec3 {
        x: 0.1,
        y: 0.2,
        z: 0.3,
    };
    draw.resolved_alpha = 0.125;

    let payload = pack_scene_material_uniforms(&storage, &[draw], 7.0);

    for (lane, expected) in [
        (0, 7.0),
        (1, 2.0),
        (2, 5.0),
        (3, 0.25),
        (4, 0.4),
        (5, 1.5),
        (6, 0.7),
        (8, 7.0),
        (9, 3.0),
        (10, 0.2),
        (12, 0.1),
        (13, 0.9),
        (14, 2.0),
        (15, 4.0),
        (16, 640.0),
        (17, 360.0),
        (18, 1.0 / 640.0),
        (19, 1.0 / 360.0),
        (20, 128.0),
        (21, 64.0),
        (22, 100.0),
        (23, 50.0),
        (24, 0.0),
        (25, 1.0),
        (26, 0.0),
        (27, 1.0),
    ] {
        assert_eq!(f32_from_payload(&payload, lane * 4), expected);
    }

    for visibility_mask in 0_u32..16 {
        draw.resolved_effect_visibility_mask = visibility_mask;
        let payload = pack_scene_material_uniforms(&storage, &[draw], 7.0);
        for stage in 0..4 {
            assert_eq!(
                f32_from_payload(&payload, (24 + stage) * 4),
                f32::from((visibility_mask & (1 << stage) != 0) as u8),
                "visibility mask {visibility_mask:#06b} changed stage {stage} identity"
            );
        }
    }

    let mut fallback = draw_with_material_visibility(SceneMaterialHandle(0), 4, 0b1111);
    fallback.authored_source_extent = [f32::NAN, 0.0];
    let fallback_payload = pack_scene_material_uniforms(&storage, &[fallback], 0.0);
    assert_eq!(f32_from_payload(&fallback_payload, 16 * 4), 1920.0);
    assert_eq!(f32_from_payload(&fallback_payload, 17 * 4), 1080.0);
}
