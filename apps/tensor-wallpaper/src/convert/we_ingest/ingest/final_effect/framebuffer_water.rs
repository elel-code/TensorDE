//! Typed framebuffer-water stages derived from the authored caustics, waterwaves, opacity, and
//! shake contracts.
//!
//! The retained graph evaluates water and opacity once per RGBA8 intermediate texel. The final
//! shake stage then uses the authored clamp-linear sampler instead of recomputing four neighboring
//! water/opacity texels in one fragment.

use super::*;

pub(super) const WATER_OPACITY_SHADER: &str = "we/framebuffer-water-quantized-water-opacity";
pub(super) const SHAKE_FINAL_SHADER: &str = "we/framebuffer-water-quantized-shake-final";

const CAUSTICS_PREPASS_SHADER: &str =
    "effects/caustics__SLOTS_3d__BLENDMODE_6__TENSOR_WALLPAPER_FRAMEBUFFER_QUANTIZED_OVERLAY_1";
const CAUSTICS_CHROMATIC_ZERO_PREPASS_SHADER: &str = "effects/caustics__SLOTS_3d__BLENDMODE_6__TENSOR_WALLPAPER_FRAMEBUFFER_QUANTIZED_OVERLAY_1__TENSOR_WALLPAPER_CHROMATIC_ZERO_1";
const CAUSTICS_CHROMATIC_ZERO_SHARED_PATTERN_PREPASS_SHADER: &str = "effects/caustics__SLOTS_3d__BLENDMODE_6__TENSOR_WALLPAPER_FRAMEBUFFER_QUANTIZED_OVERLAY_1__TENSOR_WALLPAPER_CHROMATIC_ZERO_1__TENSOR_WALLPAPER_PATTERN_GLOW_SHARED_1";
const CAUSTICS_CHROMATIC_ZERO_SHARED_PATTERN_COLOR_EQUAL_PREPASS_SHADER: &str = "effects/caustics__SLOTS_3d__BLENDMODE_6__TENSOR_WALLPAPER_FRAMEBUFFER_QUANTIZED_OVERLAY_1__TENSOR_WALLPAPER_CHROMATIC_ZERO_1__TENSOR_WALLPAPER_PATTERN_GLOW_SHARED_1__TENSOR_WALLPAPER_COLOR_EQUAL_1";

pub(super) fn source_is_supported(
    framebuffer_snapshot_available: bool,
    source_constants_are_empty: bool,
) -> bool {
    framebuffer_snapshot_available && source_constants_are_empty
}

pub(super) fn create_stages(
    builder: &mut WeIrBuilder,
    effects: &[WeEffectPassContract],
) -> Option<(WeFinalEffectPrepass, WeFinalEffectIntermediate)> {
    let caustics_effect = effects.first()?;
    let caustics = material_input(builder, caustics_effect.material_index?)?;
    if [2, 3, 4, 5]
        .into_iter()
        .any(|slot| texture_at_slot(&caustics, slot).is_none())
    {
        return None;
    }
    let caustics_shader = caustics_prepass_shader(&caustics);
    let caustics_material = push_material(
        builder,
        caustics.resource,
        caustics.pass,
        caustics_shader,
        caustics.textures,
        caustics.constants,
        ScenePipelineBlend::Normal,
    );

    let waves = material_input(builder, effects.get(1)?.material_index?)?;
    let opacity = material_input(builder, effects.get(2)?.material_index?)?;
    let mut intermediate_constants = Vec::new();
    append_effect_constants(&mut intermediate_constants, "waves", &waves);
    append_effect_constants(&mut intermediate_constants, "opacity", &opacity);
    let intermediate_material = push_material(
        builder,
        waves.resource,
        waves.pass,
        WATER_OPACITY_SHADER,
        Vec::new(),
        intermediate_constants,
        ScenePipelineBlend::Normal,
    );

    Some((
        WeFinalEffectPrepass {
            material_index: caustics_material,
            shader: caustics_shader.to_owned(),
            effect_stage_index: 0,
            input: WeFinalEffectPrepassInput::FramebufferSnapshot,
        },
        WeFinalEffectIntermediate {
            material_index: intermediate_material,
            shader: WATER_OPACITY_SHADER.to_owned(),
            effect_stage_index: 1,
            effect_stage_count: 2,
        },
    ))
}

pub(super) fn final_program(
    inputs: &[MaterialInput],
) -> Option<(
    &'static str,
    Vec<WeIrMaterialTexture>,
    Vec<WeIrMaterialConstant>,
)> {
    for slot in [2, 3, 4, 5] {
        texture_at_slot(inputs.first()?, slot)?;
    }
    let mut constants = Vec::new();
    append_effect_constants(&mut constants, "shake", inputs.get(3)?);
    let textures = vec![remap_texture(texture_at_slot(inputs.get(3)?, 1)?, 1)];
    Some((SHAKE_FINAL_SHADER, textures, constants))
}

pub(super) fn chain_is_supported(effects: &[WeEffectPassContract]) -> bool {
    let [caustics, waves, opacity, shake] = effects else {
        return false;
    };
    stage_is_replace(caustics)
        && stage_is_replace(waves)
        && stage_is_replace(opacity)
        && stage_is_replace(shake)
        && bindings_are_exact(caustics, &[0, 2, 3, 4, 5])
        && bindings_are_exact(waves, &[0])
        && bindings_are_exact(opacity, &[0])
        && bindings_are_exact(shake, &[0, 1])
        && combo_value(caustics, "BLENDMODE", -1) == 6
        && combo_value(caustics, "MODE", 0) == 0
        && combos_are_exactly_supported(caustics, &[("BLENDMODE", 6), ("MODE", 0)])
        && waves.combos.is_empty()
        && opacity.combos.is_empty()
        && shake.combos.is_empty()
}

fn caustics_prepass_shader(input: &MaterialInput) -> &'static str {
    let key = input.pass.shader_key.as_str();
    if key.contains("__TENSOR_WALLPAPER_PATTERN_GLOW_SHARED_1")
        && key.contains("__TENSOR_WALLPAPER_COLOR_EQUAL_1")
    {
        CAUSTICS_CHROMATIC_ZERO_SHARED_PATTERN_COLOR_EQUAL_PREPASS_SHADER
    } else if key.contains("__TENSOR_WALLPAPER_PATTERN_GLOW_SHARED_1") {
        CAUSTICS_CHROMATIC_ZERO_SHARED_PATTERN_PREPASS_SHADER
    } else if key.contains("__TENSOR_WALLPAPER_CHROMATIC_ZERO_1") {
        CAUSTICS_CHROMATIC_ZERO_PREPASS_SHADER
    } else {
        CAUSTICS_PREPASS_SHADER
    }
}

fn stage_is_replace(effect: &WeEffectPassContract) -> bool {
    effect
        .material_blending
        .as_deref()
        .is_none_or(|value| value == "normal")
        && effect
            .depthtest
            .as_deref()
            .is_none_or(|value| value == "disabled")
        && effect
            .depthwrite
            .as_deref()
            .is_none_or(|value| value == "disabled")
        && effect
            .cullmode
            .as_deref()
            .is_none_or(|value| value == "nocull")
}

fn bindings_are_exact(effect: &WeEffectPassContract, slots: &[u32]) -> bool {
    effect.binds.len() == slots.len()
        && slots.iter().all(|slot| {
            effect.binds.get(slot).is_some_and(|source| {
                if *slot == 0 {
                    source == "previous"
                } else {
                    !source.is_empty() && !is_graph_resource(source)
                }
            })
        })
}

fn combos_are_exactly_supported(effect: &WeEffectPassContract, supported: &[(&str, i64)]) -> bool {
    effect.combos.iter().all(|(name, value)| {
        supported.iter().any(|(supported_name, supported_value)| {
            name == supported_name && value == supported_value
        })
    })
}
