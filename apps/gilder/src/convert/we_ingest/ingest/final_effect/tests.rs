use std::collections::BTreeMap;

use super::*;

#[test]
fn only_waterripple_has_a_premultiplied_modulate_final_program() {
    assert!(final_effect_scene_blend_supported(
        FinalEffectKind::ImageWaterWaves,
        SceneBlendMode::Alpha
    ));
    assert!(final_effect_scene_blend_supported(
        FinalEffectKind::ImageWaterRipple,
        SceneBlendMode::Modulate
    ));
    assert!(!final_effect_scene_blend_supported(
        FinalEffectKind::ImageWaterWaves,
        SceneBlendMode::Modulate
    ));
}

#[test]
fn final_effect_kind_carries_its_typed_draw_primitive() {
    assert_eq!(
        final_effect_draw_primitive(FinalEffectKind::FlatRoundedOpacity),
        RenderPassDrawPrimitive::ObjectUvSupportQuad
    );
    for kind in [
        FinalEffectKind::ImageWaterWaves,
        FinalEffectKind::ImageWaterRipple,
        FinalEffectKind::ImageScroll,
        FinalEffectKind::ImageColorKeyScroll,
        FinalEffectKind::ImageCloudMotion,
        FinalEffectKind::PuppetOpacity,
        FinalEffectKind::PuppetIrisWaterRipple,
        FinalEffectKind::TechCircle,
        FinalEffectKind::AudioBars,
        FinalEffectKind::FramebufferWater,
    ] {
        assert_eq!(
            final_effect_draw_primitive(kind),
            RenderPassDrawPrimitive::ObjectMesh
        );
    }
}

#[test]
fn singleton_water_effects_are_final_draw_candidates() {
    let waves = effect("effects/waterwaves__SLOTS_3", &[0, 1]);
    assert_eq!(
        final_effect_kind("genericimage4", &[waves], true, false),
        Some(FinalEffectKind::ImageWaterWaves)
    );
    let ripple = effect("effects/waterripple__SLOTS_5", &[0, 2]);
    assert_eq!(
        final_effect_kind("genericimage4", &[ripple], true, false),
        Some(FinalEffectKind::ImageWaterRipple)
    );
}

#[test]
fn waterripple_quantized_source_prepass_requires_neutral_base_constants() {
    assert!(final_effect_source_is_supported(
        FinalEffectKind::ImageWaterRipple,
        false,
        true
    ));
    assert!(!final_effect_source_is_supported(
        FinalEffectKind::ImageWaterRipple,
        false,
        false
    ));
}

#[test]
fn singleton_cloudmotion_is_a_final_draw_candidate_with_authored_noise() {
    let cloud = effect("effects/cloudmotion__SLOTS_5", &[0, 2]);
    assert_eq!(
        final_effect_kind("genericimage4", &[cloud], true, false),
        Some(FinalEffectKind::ImageCloudMotion)
    );
}

#[test]
fn final_program_classifies_scroll_and_puppet_chains() {
    assert_eq!(
        final_effect_kind(
            "we/genericimage4",
            &[effect("effects/scroll__SLOTS_1", &[0])],
            true,
            false,
        ),
        Some(FinalEffectKind::ImageScroll)
    );
    assert_eq!(
        final_effect_kind(
            "genericimage4__PUPPETSKINNING_1",
            &[effect("effects/opacity__SLOTS_3", &[0, 1])],
            true,
            true,
        ),
        Some(FinalEffectKind::PuppetOpacity)
    );
}

#[test]
fn equivalent_composelayer_programs_are_final_draw_candidates() {
    let mut circle = effect("effects/tech_circle__SLOTS_1__SECTOR_SEGMENTS_1", &[0]);
    circle.combos.insert("SECTOR_SEGMENTS".to_owned(), 1);
    assert_eq!(
        final_effect_kind("we/composelayer", &[circle], false, false),
        Some(FinalEffectKind::TechCircle)
    );

    let mut audio = effect("effects/simple_audio_bars__SLOTS_1__SHAPE_7", &[0]);
    audio.combos.insert("SHAPE".to_owned(), 7);
    assert_eq!(
        final_effect_kind(
            "we/composelayer",
            &[
                audio,
                effect("effects/skew__SLOTS_1", &[0]),
                effect("effects/opacity__SLOTS_3", &[0, 1]),
            ],
            false,
            false,
        ),
        Some(FinalEffectKind::AudioBars)
    );
}

#[test]
fn framebuffer_lut_and_lightning_keep_authored_intermediate_target_graphs() {
    let mut lut = effect(
        "effects/lut_loader__SLOTS_3__CLAMP_0__QUAD_SIZE_64",
        &[0, 1],
    );
    lut.combos.insert("CLAMP".to_owned(), 0);
    lut.combos.insert("QUAD_SIZE".to_owned(), 64);
    assert_eq!(
        final_effect_kind("we/composelayer", &[lut], false, false),
        None
    );

    let mut lightning = effect("effects/111__SLOTS_1__BLENDMODE_7", &[0]);
    lightning.combos.insert("BLENDMODE".to_owned(), 7);
    assert_eq!(
        final_effect_kind("we/composelayer", &[lightning], false, false),
        None
    );
}

#[test]
fn strict_framebuffer_water_chain_is_a_three_stage_final_candidate() {
    let effects = framebuffer_water_effects();
    assert_eq!(
        final_effect_kind("we/composelayer", &effects, false, false),
        Some(FinalEffectKind::FramebufferWater)
    );
    assert!(final_effect_source_is_supported(
        FinalEffectKind::FramebufferWater,
        true,
        true
    ));
    assert!(!final_effect_source_is_supported(
        FinalEffectKind::FramebufferWater,
        false,
        true
    ));
    assert!(!final_effect_source_is_supported(
        FinalEffectKind::FramebufferWater,
        true,
        false
    ));
}

#[test]
fn framebuffer_water_rejects_extra_stages_combos_bindings_and_pipeline_state() {
    let mut extra_stage = framebuffer_water_effects();
    extra_stage.insert(3, effect("effects/cloudmotion__SLOTS_5", &[0, 2]));
    assert_eq!(
        final_effect_kind("we/composelayer", &extra_stage, false, false),
        None
    );

    let mut extra_combo = framebuffer_water_effects();
    extra_combo[0].combos.insert("MASK".to_owned(), 0);
    assert_eq!(
        final_effect_kind("we/composelayer", &extra_combo, false, false),
        None
    );

    let mut extra_binding = framebuffer_water_effects();
    extra_binding[3].binds.insert(2, "texture".to_owned());
    assert_eq!(
        final_effect_kind("we/composelayer", &extra_binding, false, false),
        None
    );

    let mut wrong_state = framebuffer_water_effects();
    wrong_state[1].depthtest = Some("always".to_owned());
    assert_eq!(
        final_effect_kind("we/composelayer", &wrong_state, false, false),
        None
    );

    let mut wrong_state_case = framebuffer_water_effects();
    wrong_state_case[0].material_blending = Some("Normal".to_owned());
    wrong_state_case[1].depthtest = Some("Disabled".to_owned());
    wrong_state_case[2].depthwrite = Some("Disabled".to_owned());
    wrong_state_case[3].cullmode = Some("NoCull".to_owned());
    assert_eq!(
        final_effect_kind("we/composelayer", &wrong_state_case, false, false),
        None
    );

    let mut wrong_combo_case = framebuffer_water_effects();
    let blendmode = wrong_combo_case[0]
        .combos
        .remove("BLENDMODE")
        .expect("authored blend combo");
    wrong_combo_case[0]
        .combos
        .insert("blendmode".to_owned(), blendmode);
    assert_eq!(
        final_effect_kind("we/composelayer", &wrong_combo_case, false, false),
        None
    );

    let mut previous_alias = framebuffer_water_effects();
    previous_alias[2].binds.insert(0, "_previous".to_owned());
    assert_eq!(
        final_effect_kind("we/composelayer", &previous_alias, false, false),
        None
    );
}

fn framebuffer_water_effects() -> Vec<WeEffectPassContract> {
    let mut caustics = effect("effects/caustics__SLOTS_3d__BLENDMODE_6", &[0, 2, 3, 4, 5]);
    caustics.combos.insert("BLENDMODE".to_owned(), 6);
    vec![
        caustics,
        effect("effects/waterwaves__SLOTS_1", &[0]),
        effect("effects/opacity__SLOTS_1", &[0]),
        effect("effects/shake__SLOTS_3", &[0, 1]),
    ]
}

fn effect(shader: &str, slots: &[u32]) -> WeEffectPassContract {
    WeEffectPassContract {
        object_index: 1,
        effect_binding_start: 0,
        effect_binding_count: 1,
        runtime_visibility: true,
        material_index: Some(2),
        effect_file: "effect.json".to_owned(),
        pass_index: 0,
        command: None,
        shader: Some(shader.to_owned()),
        source: None,
        target: None,
        binds: slots
            .iter()
            .copied()
            .map(|slot| {
                (
                    slot,
                    if slot == 0 { "previous" } else { "texture" }.to_owned(),
                )
            })
            .collect(),
        pass_constants: Vec::new(),
        material_blending: Some("normal".to_owned()),
        depthtest: None,
        depthwrite: None,
        cullmode: None,
        combos: BTreeMap::new(),
    }
}
