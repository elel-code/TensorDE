//! Converter-owned final-draw materials for independently sampled image effects.
//!
//! These typed materials remove per-object source/effect/composite target chains. The GPU
//! evaluates the effect while producing the final scene-color fragment.

use crate::convert::we_ingest::ir::{
    WeIrMaterial, WeIrMaterialConstant, WeIrMaterialPass, WeIrMaterialTexture,
};
use crate::core::SceneBlendMode;
use crate::engine::render_graph::{
    WeEffectPassContract, WeFinalEffectMaterial, WeFinalEffectPrepass,
};
use crate::engine::scene::{SceneCullMode, SceneDepthTest, ScenePipelineBlend};

use super::WeIrBuilder;

pub(super) const IMAGE_WATERWAVES_FINAL_SHADER: &str = "we/image-waterwaves-final";
pub(super) const IMAGE_WATERRIPPLE_FINAL_SHADER: &str = "we/image-waterripple-final";
pub(super) const IMAGE_WATERRIPPLE_MODULATE_FINAL_SHADER: &str =
    "we/image-waterripple-modulate-final";
pub(super) const IMAGE_SCROLL_FINAL_SHADER: &str = "we/image-scroll-final";
pub(super) const IMAGE_COLORKEY_SCROLL_FINAL_SHADER: &str = "we/image-colorkey-scroll-final";
pub(super) const IMAGE_CLOUD_MOTION_FINAL_SHADER: &str = "we/image-cloudmotion-final";
pub(super) const PUPPET_OPACITY_FINAL_SHADER: &str = "we/puppet-opacity-final";
pub(super) const PUPPET_IRIS_WATERRIPPLE_FINAL_SHADER: &str = "we/puppet-iris-waterripple-final";
pub(super) const FLAT_ROUNDED_OPACITY_FINAL_SHADER: &str = "we/flat-rounded-opacity-final";
pub(super) const TECH_CIRCLE_FINAL_SHADER: &str = "we/tech-circle-final";
pub(super) const AUDIO_BARS_FINAL_SHADER: &str = "we/audio-bars-final";
pub(super) const FRAMEBUFFER_WATER_QUANTIZED_FINAL_SHADER: &str =
    "we/framebuffer-water-quantized-final";
const FRAMEBUFFER_CAUSTICS_QUANTIZED_PREPASS_SHADER: &str =
    "effects/caustics__SLOTS_3d__BLENDMODE_6__GILDER_FRAMEBUFFER_QUANTIZED_OVERLAY_1";
const FRAMEBUFFER_CAUSTICS_CHROMATIC_ZERO_QUANTIZED_PREPASS_SHADER: &str = "effects/caustics__SLOTS_3d__BLENDMODE_6__GILDER_FRAMEBUFFER_QUANTIZED_OVERLAY_1__GILDER_CHROMATIC_ZERO_1";
const FRAMEBUFFER_CAUSTICS_CHROMATIC_ZERO_SHARED_PATTERN_QUANTIZED_PREPASS_SHADER: &str = "effects/caustics__SLOTS_3d__BLENDMODE_6__GILDER_FRAMEBUFFER_QUANTIZED_OVERLAY_1__GILDER_CHROMATIC_ZERO_1__GILDER_PATTERN_GLOW_SHARED_1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FinalEffectKind {
    ImageWaterWaves,
    ImageWaterRipple,
    ImageScroll,
    ImageColorKeyScroll,
    ImageCloudMotion,
    PuppetOpacity,
    PuppetIrisWaterRipple,
    FlatRoundedOpacity,
    TechCircle,
    AudioBars,
    FramebufferWater,
}

#[derive(Debug, Clone)]
struct MaterialInput {
    resource: u32,
    pass: WeIrMaterialPass,
    textures: Vec<WeIrMaterialTexture>,
    constants: Vec<WeIrMaterialConstant>,
}

pub(super) fn create(
    builder: &mut WeIrBuilder,
    base_material_handle: u32,
    effects: &[WeEffectPassContract],
    final_scene_blend: SceneBlendMode,
    effects_in_authored_texture_space: bool,
    object_is_puppet: bool,
    framebuffer_snapshot_available: bool,
) -> Option<WeFinalEffectMaterial> {
    create_for_kind(
        builder,
        base_material_handle,
        effects,
        final_scene_blend,
        effects_in_authored_texture_space,
        object_is_puppet,
        framebuffer_snapshot_available,
        None,
    )
}

pub(super) fn create_framebuffer_water(
    builder: &mut WeIrBuilder,
    base_material_handle: u32,
    effects: &[WeEffectPassContract],
    final_scene_blend: SceneBlendMode,
    framebuffer_snapshot_available: bool,
) -> Option<WeFinalEffectMaterial> {
    create_for_kind(
        builder,
        base_material_handle,
        effects,
        final_scene_blend,
        false,
        false,
        framebuffer_snapshot_available,
        Some(FinalEffectKind::FramebufferWater),
    )
}

#[allow(clippy::too_many_arguments)]
fn create_for_kind(
    builder: &mut WeIrBuilder,
    base_material_handle: u32,
    effects: &[WeEffectPassContract],
    final_scene_blend: SceneBlendMode,
    effects_in_authored_texture_space: bool,
    object_is_puppet: bool,
    framebuffer_snapshot_available: bool,
    required_kind: Option<FinalEffectKind>,
) -> Option<WeFinalEffectMaterial> {
    let source = material_input(builder, base_material_handle as usize)?;
    if source
        .textures
        .iter()
        .any(|texture| texture.slot != 0 && texture_is_bound(texture))
    {
        return None;
    }
    let kind = final_effect_kind(
        &source.pass.shader_key,
        effects,
        effects_in_authored_texture_space,
        object_is_puppet,
    )?;
    if required_kind.is_some_and(|required| required != kind)
        || !final_effect_source_is_supported(
            kind,
            framebuffer_snapshot_available,
            source.constants.is_empty(),
        )
    {
        return None;
    }
    if !final_effect_scene_blend_supported(kind, final_scene_blend) {
        return None;
    }
    let (shader, textures, constants) =
        final_effect_program(builder, &source, effects, kind, final_scene_blend)?;
    let prepass = framebuffer_caustics_prepass(builder, effects, kind)?;
    let material_index = push_material(
        builder,
        source.resource,
        source.pass,
        shader,
        textures,
        constants,
        ScenePipelineBlend::Translucent,
    );
    Some(WeFinalEffectMaterial {
        material_index,
        shader: shader.to_owned(),
        prepass,
    })
}

fn final_effect_source_is_supported(
    kind: FinalEffectKind,
    framebuffer_snapshot_available: bool,
    source_constants_are_empty: bool,
) -> bool {
    kind != FinalEffectKind::FramebufferWater
        || (framebuffer_snapshot_available && source_constants_are_empty)
}

fn framebuffer_caustics_prepass(
    builder: &mut WeIrBuilder,
    effects: &[WeEffectPassContract],
    kind: FinalEffectKind,
) -> Option<Option<WeFinalEffectPrepass>> {
    if kind != FinalEffectKind::FramebufferWater {
        return Some(None);
    }
    let effect = effects.first()?;
    let input = material_input(builder, effect.material_index?)?;
    if [2, 3, 4, 5]
        .into_iter()
        .any(|slot| texture_at_slot(&input, slot).is_none())
    {
        return None;
    }
    let shader = framebuffer_caustics_prepass_shader(&input);
    let material_index = push_material(
        builder,
        input.resource,
        input.pass,
        shader,
        input.textures,
        input.constants,
        ScenePipelineBlend::Normal,
    );
    Some(Some(WeFinalEffectPrepass {
        material_index,
        shader: shader.to_owned(),
        effect_stage_index: 0,
    }))
}

fn framebuffer_caustics_prepass_shader(input: &MaterialInput) -> &'static str {
    if input
        .pass
        .shader_key
        .contains("__GILDER_PATTERN_GLOW_SHARED_1")
    {
        FRAMEBUFFER_CAUSTICS_CHROMATIC_ZERO_SHARED_PATTERN_QUANTIZED_PREPASS_SHADER
    } else if input.pass.shader_key.contains("__GILDER_CHROMATIC_ZERO_1") {
        FRAMEBUFFER_CAUSTICS_CHROMATIC_ZERO_QUANTIZED_PREPASS_SHADER
    } else {
        FRAMEBUFFER_CAUSTICS_QUANTIZED_PREPASS_SHADER
    }
}

fn final_effect_scene_blend_supported(kind: FinalEffectKind, scene_blend: SceneBlendMode) -> bool {
    scene_blend == SceneBlendMode::Alpha
        || (kind == FinalEffectKind::ImageWaterRipple && scene_blend == SceneBlendMode::Modulate)
}

fn final_effect_program(
    builder: &WeIrBuilder,
    source: &MaterialInput,
    effects: &[WeEffectPassContract],
    kind: FinalEffectKind,
    final_scene_blend: SceneBlendMode,
) -> Option<(
    &'static str,
    Vec<WeIrMaterialTexture>,
    Vec<WeIrMaterialConstant>,
)> {
    let inputs = effects
        .iter()
        .map(|effect| material_input(builder, effect.material_index?))
        .collect::<Option<Vec<_>>>()?;
    let mut textures = match kind {
        FinalEffectKind::FlatRoundedOpacity
        | FinalEffectKind::TechCircle
        | FinalEffectKind::FramebufferWater => Vec::new(),
        FinalEffectKind::AudioBars => texture_at_slot(&inputs[2], 1)
            .map(|texture| vec![remap_texture(texture, 0)])
            .unwrap_or_default(),
        _ => vec![source_texture(source)?],
    };
    let mut constants = if kind == FinalEffectKind::FramebufferWater {
        Vec::new()
    } else {
        prefixed_constants("base", &source.constants)
    };
    let shader = match kind {
        FinalEffectKind::ImageWaterWaves => {
            append_effect_constants(&mut constants, "effect", &inputs[0]);
            append_optional_texture(&mut textures, &inputs[0], 1, 1);
            constants.push(synthetic_constant(
                "effect.mask_enabled",
                texture_at_slot(&inputs[0], 1).is_some(),
            ));
            constants.push(synthetic_constant(
                "effect.dualwaves",
                combo_enabled(&effects[0], "DUALWAVES"),
            ));
            IMAGE_WATERWAVES_FINAL_SHADER
        }
        FinalEffectKind::ImageWaterRipple => {
            append_effect_constants(&mut constants, "effect", &inputs[0]);
            append_optional_texture(&mut textures, &inputs[0], 1, 1);
            textures.push(remap_texture(texture_at_slot(&inputs[0], 2)?, 2));
            constants.push(synthetic_constant(
                "effect.mask_enabled",
                texture_at_slot(&inputs[0], 1).is_some(),
            ));
            if final_scene_blend == SceneBlendMode::Modulate {
                IMAGE_WATERRIPPLE_MODULATE_FINAL_SHADER
            } else {
                IMAGE_WATERRIPPLE_FINAL_SHADER
            }
        }
        FinalEffectKind::ImageScroll => {
            append_effect_constants(&mut constants, "scroll", &inputs[0]);
            IMAGE_SCROLL_FINAL_SHADER
        }
        FinalEffectKind::ImageColorKeyScroll => {
            append_effect_constants(&mut constants, "colorkey", &inputs[0]);
            append_effect_constants(&mut constants, "scroll", &inputs[1]);
            constants.push(synthetic_constant(
                "colorkey.invert",
                combo_enabled(&effects[0], "INVERT"),
            ));
            constants.push(synthetic_constant(
                "colorkey.flatten",
                combo_enabled(&effects[0], "FLATTEN"),
            ));
            IMAGE_COLORKEY_SCROLL_FINAL_SHADER
        }
        FinalEffectKind::ImageCloudMotion => {
            append_effect_constants(&mut constants, "cloud", &inputs[0]);
            textures.push(remap_texture(texture_at_slot(&inputs[0], 2)?, 2));
            IMAGE_CLOUD_MOTION_FINAL_SHADER
        }
        FinalEffectKind::PuppetOpacity => {
            append_effect_constants(&mut constants, "opacity", &inputs[0]);
            append_optional_texture(&mut textures, &inputs[0], 1, 1);
            constants.push(synthetic_constant(
                "opacity.mask_enabled",
                texture_at_slot(&inputs[0], 1).is_some(),
            ));
            PUPPET_OPACITY_FINAL_SHADER
        }
        FinalEffectKind::PuppetIrisWaterRipple => {
            append_effect_constants(&mut constants, "iris", &inputs[0]);
            append_effect_constants(&mut constants, "ripple", &inputs[1]);
            append_optional_texture(&mut textures, &inputs[0], 1, 1);
            append_optional_texture(&mut textures, &inputs[1], 1, 2);
            append_optional_texture(&mut textures, &inputs[1], 2, 3);
            constants.extend([
                synthetic_constant(
                    "iris.mask_enabled",
                    texture_at_slot(&inputs[0], 1).is_some(),
                ),
                synthetic_constant("iris.background", combo_enabled(&effects[0], "BACKGROUND")),
                synthetic_constant(
                    "ripple.mask_enabled",
                    texture_at_slot(&inputs[1], 1).is_some()
                        && texture_at_slot(&inputs[1], 2).is_some(),
                ),
                synthetic_constant(
                    "ripple.normal_enabled",
                    texture_at_slot(&inputs[1], 2).is_some(),
                ),
            ]);
            PUPPET_IRIS_WATERRIPPLE_FINAL_SHADER
        }
        FinalEffectKind::FlatRoundedOpacity => {
            append_effect_constants(&mut constants, "rounded", &inputs[0]);
            append_effect_constants(&mut constants, "opacity", &inputs[1]);
            FLAT_ROUNDED_OPACITY_FINAL_SHADER
        }
        FinalEffectKind::TechCircle => {
            append_effect_constants(&mut constants, "tech", &inputs[0]);
            TECH_CIRCLE_FINAL_SHADER
        }
        FinalEffectKind::AudioBars => {
            append_effect_constants(&mut constants, "audio", &inputs[0]);
            append_effect_constants(&mut constants, "skew", &inputs[1]);
            append_effect_constants(&mut constants, "opacity", &inputs[2]);
            constants.push(synthetic_constant(
                "opacity.mask_enabled",
                texture_at_slot(&inputs[2], 1).is_some(),
            ));
            AUDIO_BARS_FINAL_SHADER
        }
        FinalEffectKind::FramebufferWater => {
            for slot in [2, 3, 4, 5] {
                texture_at_slot(&inputs[0], slot)?;
            }
            append_effect_constants(&mut constants, "waves", &inputs[1]);
            append_effect_constants(&mut constants, "opacity", &inputs[2]);
            append_effect_constants(&mut constants, "shake", &inputs[3]);
            textures.push(remap_texture(texture_at_slot(&inputs[3], 1)?, 1));
            FRAMEBUFFER_WATER_QUANTIZED_FINAL_SHADER
        }
    };
    Some((shader, textures, constants))
}

fn final_effect_kind(
    base_shader: &str,
    effects: &[WeEffectPassContract],
    effects_in_authored_texture_space: bool,
    object_is_puppet: bool,
) -> Option<FinalEffectKind> {
    let effect_names = effects
        .iter()
        .map(compatible_effect_name)
        .collect::<Option<Vec<_>>>()?;
    let effect_names = effect_names.iter().map(String::as_str).collect::<Vec<_>>();
    if is_flat_shader(base_shader)
        && effect_names.as_slice() == ["rounded_mask", "opacity"]
        && rounded_opacity_chain_is_supported(effects)
    {
        return Some(FinalEffectKind::FlatRoundedOpacity);
    }
    if is_composelayer_shader(base_shader)
        && effect_names.as_slice() == ["tech_circle"]
        && tech_circle_is_supported(&effects[0])
    {
        return Some(FinalEffectKind::TechCircle);
    }
    if is_composelayer_shader(base_shader)
        && effect_names.as_slice() == ["simple_audio_bars", "skew", "opacity"]
        && audio_bars_is_supported(&effects[0])
        && skew_is_supported(&effects[1])
        && previous_only(&effects[2], &[0, 1])
    {
        return Some(FinalEffectKind::AudioBars);
    }
    if is_composelayer_shader(base_shader)
        && effect_names.as_slice() == ["caustics", "waterwaves", "opacity", "shake"]
        && framebuffer_water_chain_is_supported(effects)
    {
        return Some(FinalEffectKind::FramebufferWater);
    }
    if !effects_in_authored_texture_space || !is_generic_image_shader(base_shader) {
        return None;
    }
    match (object_is_puppet, effect_names.as_slice()) {
        (false, ["waterwaves"]) if waterwaves_is_supported(&effects[0]) => {
            Some(FinalEffectKind::ImageWaterWaves)
        }
        (false, ["waterripple"]) if waterripple_is_supported(&effects[0]) => {
            Some(FinalEffectKind::ImageWaterRipple)
        }
        (false, ["scroll"]) if previous_only(&effects[0], &[0]) => {
            Some(FinalEffectKind::ImageScroll)
        }
        (false, ["colorkey", "scroll"])
            if previous_only(&effects[0], &[0]) && previous_only(&effects[1], &[0]) =>
        {
            Some(FinalEffectKind::ImageColorKeyScroll)
        }
        (false, ["cloudmotion"]) if cloudmotion_is_supported(&effects[0]) => {
            Some(FinalEffectKind::ImageCloudMotion)
        }
        (true, ["opacity"]) if previous_only(&effects[0], &[0, 1]) => {
            Some(FinalEffectKind::PuppetOpacity)
        }
        (true, ["iris", "waterripple"])
            if iris_is_supported(&effects[0]) && waterripple_is_supported(&effects[1]) =>
        {
            Some(FinalEffectKind::PuppetIrisWaterRipple)
        }
        _ => None,
    }
}

fn compatible_effect_name(effect: &WeEffectPassContract) -> Option<String> {
    if effect.command.is_some()
        || effect.source.is_some()
        || effect.target.is_some()
        || effect.material_index.is_none()
        || !effect
            .binds
            .get(&0)
            .is_some_and(|source| is_previous_source(source))
    {
        return None;
    }
    effect.shader.as_deref().map(shader_basename)
}

fn previous_only(effect: &WeEffectPassContract, slots: &[u32]) -> bool {
    bindings_are_local(effect, slots)
        && effect
            .material_blending
            .as_deref()
            .is_none_or(|blend| blend.eq_ignore_ascii_case("normal"))
}

fn waterwaves_is_supported(effect: &WeEffectPassContract) -> bool {
    previous_only(effect, &[0, 1])
        && !combo_enabled(effect, "PERSPECTIVE")
        && !combo_enabled(effect, "TIMEOFFSET")
}

fn waterripple_is_supported(effect: &WeEffectPassContract) -> bool {
    previous_only(effect, &[0, 1, 2])
        && effect.binds.contains_key(&2)
        && !combo_enabled(effect, "PERSPECTIVE")
        && !combo_enabled(effect, "SPECULAR")
}

fn cloudmotion_is_supported(effect: &WeEffectPassContract) -> bool {
    previous_only(effect, &[0, 2]) && effect.binds.contains_key(&2)
}

fn iris_is_supported(effect: &WeEffectPassContract) -> bool {
    previous_only(effect, &[0, 1]) && !combo_enabled(effect, "PERSPECTIVE")
}

fn rounded_opacity_chain_is_supported(effects: &[WeEffectPassContract]) -> bool {
    previous_only(&effects[0], &[0])
        && previous_only(&effects[1], &[0])
        && effects[0].combos.get("B_SQUARE") == Some(&0)
        && effects[0].combos.get("C_ALPHA_ONLY") == Some(&0)
        && effects[0].combos.get("SOFT") == Some(&1)
}

fn tech_circle_is_supported(effect: &WeEffectPassContract) -> bool {
    previous_only(effect, &[0])
        && combo_value(effect, "COORD_SYS", 1) == 1
        && combo_value(effect, "RING_SEGMENTS", 0) == 0
        && combo_value(effect, "SECTOR_SEGMENTS", 0) == 1
        && combo_value(effect, "RATIO_CORRECTION", 0) == 0
}

fn audio_bars_is_supported(effect: &WeEffectPassContract) -> bool {
    previous_only(effect, &[0]) && combo_value(effect, "SHAPE", 0) == 7
}

fn skew_is_supported(effect: &WeEffectPassContract) -> bool {
    previous_only(effect, &[0]) && combo_value(effect, "REPEAT", 1) != 0
}

fn framebuffer_water_chain_is_supported(effects: &[WeEffectPassContract]) -> bool {
    let [caustics, waves, opacity, shake] = effects else {
        return false;
    };
    framebuffer_stage_is_replace(caustics)
        && framebuffer_stage_is_replace(waves)
        && framebuffer_stage_is_replace(opacity)
        && framebuffer_stage_is_replace(shake)
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

fn framebuffer_stage_is_replace(effect: &WeEffectPassContract) -> bool {
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

fn combo_value(effect: &WeEffectPassContract, name: &str, default: i64) -> i64 {
    effect.combos.get(name).copied().unwrap_or(default)
}

fn bindings_are_local(effect: &WeEffectPassContract, allowed_slots: &[u32]) -> bool {
    effect.binds.iter().all(|(slot, source)| {
        allowed_slots.contains(slot) && (*slot == 0 || !is_graph_resource(source))
    })
}

fn is_previous_source(source: &str) -> bool {
    matches!(source, "previous" | "_previous" | "$previous")
}

fn is_graph_resource(source: &str) -> bool {
    is_previous_source(source)
        || source.eq_ignore_ascii_case("source")
        || source.starts_with("fbo_")
        || source.starts_with("_rt_")
        || source.starts_with("_alias_")
}

fn combo_enabled(effect: &WeEffectPassContract, name: &str) -> bool {
    effect
        .combos
        .iter()
        .any(|(candidate, value)| candidate.eq_ignore_ascii_case(name) && *value != 0)
}

fn is_generic_image_shader(shader: &str) -> bool {
    matches!(
        shader_basename(shader).as_str(),
        "genericimage2" | "genericimage4"
    )
}

fn is_flat_shader(shader: &str) -> bool {
    shader_basename(shader) == "flat"
}

fn is_composelayer_shader(shader: &str) -> bool {
    shader_basename(shader) == "composelayer"
}

fn shader_basename(shader: &str) -> String {
    shader
        .split("__")
        .next()
        .unwrap_or_default()
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn material_input(builder: &WeIrBuilder, material_index: usize) -> Option<MaterialInput> {
    let material = builder.materials.get(material_index)?;
    let pass = builder
        .material_passes
        .get(material.pass_start as usize)?
        .clone();
    let textures = builder
        .material_textures
        .get(
            pass.texture_start as usize
                ..pass.texture_start.saturating_add(pass.texture_count) as usize,
        )?
        .to_vec();
    let constants = builder
        .material_constants
        .get(
            pass.constant_start as usize
                ..pass.constant_start.saturating_add(pass.constant_count) as usize,
        )?
        .to_vec();
    Some(MaterialInput {
        resource: material.resource,
        pass,
        textures,
        constants,
    })
}

fn texture_at_slot(input: &MaterialInput, slot: u32) -> Option<&WeIrMaterialTexture> {
    input
        .textures
        .iter()
        .find(|texture| texture.slot == slot && texture_is_bound(texture))
}

fn source_texture(source: &MaterialInput) -> Option<WeIrMaterialTexture> {
    texture_at_slot(source, 0).map(|texture| remap_texture(texture, 0))
}

fn append_optional_texture(
    textures: &mut Vec<WeIrMaterialTexture>,
    input: &MaterialInput,
    source_slot: u32,
    destination_slot: u32,
) {
    if let Some(texture) = texture_at_slot(input, source_slot) {
        textures.push(remap_texture(texture, destination_slot));
    }
}

fn append_effect_constants(
    constants: &mut Vec<WeIrMaterialConstant>,
    prefix: &str,
    input: &MaterialInput,
) {
    constants.extend(prefixed_constants(prefix, &input.constants));
}

fn texture_is_bound(texture: &WeIrMaterialTexture) -> bool {
    texture.resource.is_some() || !texture.path.is_empty()
}

fn remap_texture(texture: &WeIrMaterialTexture, slot: u32) -> WeIrMaterialTexture {
    WeIrMaterialTexture {
        slot,
        resource: texture.resource,
        path: texture.path.clone(),
    }
}

fn prefixed_constants(
    prefix: &str,
    constants: &[WeIrMaterialConstant],
) -> Vec<WeIrMaterialConstant> {
    constants
        .iter()
        .map(|constant| WeIrMaterialConstant {
            name: format!("{prefix}.{}", constant.name),
            value_json: constant.value_json.clone(),
        })
        .collect()
}

fn synthetic_constant(name: &str, enabled: bool) -> WeIrMaterialConstant {
    WeIrMaterialConstant {
        name: name.to_owned(),
        value_json: if enabled { "1" } else { "0" }.to_owned(),
    }
}

fn push_material(
    builder: &mut WeIrBuilder,
    resource: u32,
    mut pass: WeIrMaterialPass,
    shader: &str,
    textures: Vec<WeIrMaterialTexture>,
    constants: Vec<WeIrMaterialConstant>,
    pipeline_blend: ScenePipelineBlend,
) -> usize {
    let handle = builder.materials.len() as u32;
    let texture_start = builder.material_textures.len() as u32;
    builder.material_textures.extend(textures);
    let constant_start = builder.material_constants.len() as u32;
    builder.material_constants.extend(constants);
    pass.material = handle;
    pass.shader_key = shader.to_owned();
    pass.target.clear();
    pass.texture_start = texture_start;
    pass.texture_count = builder.material_textures.len() as u32 - texture_start;
    pass.constant_start = constant_start;
    pass.constant_count = builder.material_constants.len() as u32 - constant_start;
    pass.pipeline_blend = pipeline_blend;
    pass.depth_test = SceneDepthTest::Disabled;
    pass.depth_write = false;
    pass.cull_mode = SceneCullMode::None;
    pass.clear_target = false;
    let pass_start = builder.material_passes.len() as u32;
    builder.material_passes.push(pass);
    builder.materials.push(WeIrMaterial {
        handle,
        resource,
        pass_start,
        pass_count: 1,
    });
    handle as usize
}

#[cfg(test)]
mod tests {
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
    fn strict_framebuffer_water_chain_is_a_two_stage_final_candidate() {
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
}
