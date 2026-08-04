//! Converter-owned aggregate material for a typed direct foliage/ripple composite.

use crate::convert::we_ingest::ir::{
    WeIrMaterial, WeIrMaterialConstant, WeIrMaterialPass, WeIrMaterialTexture,
};
use crate::core::SceneBlendMode;
use crate::engine::render_graph::{
    WeEffectPassContract, WeFoliageRippleMaterial, we_effect_passes_form_foliage_ripple_chain,
};
use crate::engine::scene::{SceneCullMode, SceneDepthTest, ScenePipelineBlend};

use super::WeIrBuilder;

pub(super) const FOLIAGE_RIPPLE_SHADER: &str = "we/image-foliage-ripple-composite";
pub(super) const FOLIAGE_RIPPLE_SCREEN_SHADER: &str = "we/image-foliage-ripple-screen-composite";
const FOLIAGE_RIPPLE_POWER_TWO_SHADER: &str =
    "we/image-foliage-ripple-composite__TENSOR_WALLPAPER_FOLIAGE_POWER_TWO_1";
const FOLIAGE_RIPPLE_SCREEN_POWER_TWO_SHADER: &str =
    "we/image-foliage-ripple-screen-composite__TENSOR_WALLPAPER_FOLIAGE_POWER_TWO_1";

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
) -> Option<WeFoliageRippleMaterial> {
    if !we_effect_passes_form_foliage_ripple_chain(effects) {
        return None;
    }
    let source = material_input(builder, base_material_handle as usize)?;
    let foliage = material_input(builder, effects[0].material_index?)?;
    let ripple = material_input(builder, effects[1].material_index?)?;
    if source
        .textures
        .iter()
        .any(|texture| texture.slot != 0 && texture_is_bound(texture))
        || foliage
            .textures
            .iter()
            .any(|texture| texture.slot == 1 && texture_is_bound(texture))
        || ripple
            .textures
            .iter()
            .any(|texture| texture.slot == 1 && texture_is_bound(texture))
    {
        return None;
    }
    let source_texture = texture_at_slot(&source, 0)?;
    let noise_texture = texture_at_slot(&foliage, 2)?;
    let normal_texture = texture_at_slot(&ripple, 2)?;

    let handle = builder.materials.len() as u32;
    let texture_start = builder.material_textures.len() as u32;
    for (slot, texture) in [(0, source_texture), (1, noise_texture), (3, normal_texture)] {
        let mut texture = texture.clone();
        texture.slot = slot;
        builder.material_textures.push(texture);
    }
    let constant_start = builder.material_constants.len() as u32;
    append_constants(builder, "base", &source.constants);
    append_constants(builder, "foliage", &foliage.constants);
    append_constants(builder, "ripple", &ripple.constants);

    let mut pass = source.pass;
    pass.material = handle;
    let foliage_power_two = constant_is_static_number(&foliage.constants, "power", 2.0);
    let shader = shader_for_scene_blend(final_scene_blend, foliage_power_two).to_owned();
    pass.shader_key = shader.clone();
    pass.target.clear();
    pass.texture_start = texture_start;
    pass.texture_count = builder.material_textures.len() as u32 - texture_start;
    pass.constant_start = constant_start;
    pass.constant_count = builder.material_constants.len() as u32 - constant_start;
    pass.pipeline_blend = ScenePipelineBlend::Translucent;
    pass.depth_test = SceneDepthTest::Disabled;
    pass.depth_write = false;
    pass.cull_mode = SceneCullMode::None;
    pass.clear_target = false;
    let pass_start = builder.material_passes.len() as u32;
    builder.material_passes.push(pass);
    builder.materials.push(WeIrMaterial {
        handle,
        resource: source.resource,
        pass_start,
        pass_count: 1,
    });
    Some(WeFoliageRippleMaterial {
        material_index: handle as usize,
        shader,
    })
}

fn shader_for_scene_blend(blend: SceneBlendMode, foliage_power_two: bool) -> &'static str {
    match (blend == SceneBlendMode::Screen, foliage_power_two) {
        (true, true) => FOLIAGE_RIPPLE_SCREEN_POWER_TWO_SHADER,
        (true, false) => FOLIAGE_RIPPLE_SCREEN_SHADER,
        (false, true) => FOLIAGE_RIPPLE_POWER_TWO_SHADER,
        (false, false) => FOLIAGE_RIPPLE_SHADER,
    }
}

fn constant_is_static_number(
    constants: &[WeIrMaterialConstant],
    name: &str,
    expected: f32,
) -> bool {
    constants
        .iter()
        .find(|constant| constant.name.eq_ignore_ascii_case(name))
        .and_then(|constant| constant.value_json.trim().parse::<f32>().ok())
        .is_some_and(|value| value.is_finite() && (value - expected).abs() <= 1.0e-7)
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

fn texture_is_bound(texture: &WeIrMaterialTexture) -> bool {
    texture.resource.is_some() || !texture.path.is_empty()
}

fn append_constants(builder: &mut WeIrBuilder, stage: &str, constants: &[WeIrMaterialConstant]) {
    builder
        .material_constants
        .extend(constants.iter().map(|constant| WeIrMaterialConstant {
            name: format!("{stage}.{}", constant.name),
            value_json: constant.value_json.clone(),
        }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_material_uses_the_typed_screen_shader() {
        assert_eq!(
            shader_for_scene_blend(SceneBlendMode::Screen, false),
            FOLIAGE_RIPPLE_SCREEN_SHADER
        );
        assert_eq!(
            shader_for_scene_blend(SceneBlendMode::Alpha, false),
            FOLIAGE_RIPPLE_SHADER
        );
        assert_eq!(
            shader_for_scene_blend(SceneBlendMode::Screen, true),
            FOLIAGE_RIPPLE_SCREEN_POWER_TWO_SHADER
        );
    }

    #[test]
    fn power_two_variant_requires_a_static_numeric_constant() {
        let numeric = [WeIrMaterialConstant {
            name: "power".to_owned(),
            value_json: "2".to_owned(),
        }];
        let property = [WeIrMaterialConstant {
            name: "power".to_owned(),
            value_json: r#"{"user":"foliage_power"}"#.to_owned(),
        }];
        assert!(constant_is_static_number(&numeric, "power", 2.0));
        assert!(!constant_is_static_number(&property, "power", 2.0));
    }
}
