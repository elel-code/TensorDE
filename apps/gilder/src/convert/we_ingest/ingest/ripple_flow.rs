//! Converter-owned materials for a typed ripple then flow chain.

use crate::convert::we_ingest::ir::{
    WeIrMaterial, WeIrMaterialConstant, WeIrMaterialPass, WeIrMaterialTexture,
};
use crate::core::SceneBlendMode;
use crate::engine::render_graph::{
    WeEffectPassContract, WeRippleFlowMaterialIndices, we_effect_passes_form_ripple_flow_chain,
};
use crate::engine::scene::{SceneCullMode, SceneDepthTest, ScenePipelineBlend};

use super::WeIrBuilder;

pub(super) const RIPPLE_SOURCE_SHADER: &str = "we/image-ripple-source";
pub(super) const RIPPLE_FLOW_COMPOSITE_SHADER: &str = "we/image-ripple-flow-composite";
pub(super) const RIPPLE_FLOW_MULTIPLY_COMPOSITE_SHADER: &str =
    "we/image-ripple-flow-multiply-composite";

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
) -> Option<WeRippleFlowMaterialIndices> {
    if !we_effect_passes_form_ripple_flow_chain(effects) {
        return None;
    }
    let source = material_input(builder, base_material_handle as usize)?;
    let ripple = material_input(builder, effects[0].material_index?)?;
    let flow = material_input(builder, effects[1].material_index?)?;
    if source
        .textures
        .iter()
        .any(|texture| texture.slot != 0 && texture_is_bound(texture))
    {
        return None;
    }
    let ripple_source = push_material(
        builder,
        source.resource,
        ripple.pass.clone(),
        RIPPLE_SOURCE_SHADER,
        vec![
            remap_texture(texture_at_slot(&source, 0)?, 0),
            remap_texture(texture_at_slot(&ripple, 2)?, 2),
        ],
        ripple.constants.clone(),
        ScenePipelineBlend::Normal,
    );
    let flow_composite = push_material(
        builder,
        source.resource,
        flow.pass.clone(),
        flow_composite_shader(final_scene_blend),
        vec![
            remap_texture(texture_at_slot(&flow, 1)?, 1),
            remap_texture(texture_at_slot(&flow, 2)?, 2),
        ],
        prefixed_constants("base", &source.constants)
            .into_iter()
            .chain(prefixed_constants("flow", &flow.constants))
            .collect(),
        ScenePipelineBlend::Translucent,
    );
    Some(WeRippleFlowMaterialIndices {
        ripple_source,
        flow_composite,
    })
}

fn flow_composite_shader(scene_blend: SceneBlendMode) -> &'static str {
    if scene_blend == SceneBlendMode::Multiply {
        RIPPLE_FLOW_MULTIPLY_COMPOSITE_SHADER
    } else {
        RIPPLE_FLOW_COMPOSITE_SHADER
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

fn remap_texture(texture: &WeIrMaterialTexture, slot: u32) -> WeIrMaterialTexture {
    WeIrMaterialTexture {
        slot,
        resource: texture.resource,
        path: texture.path.clone(),
    }
}

fn prefixed_constants(
    stage: &str,
    constants: &[WeIrMaterialConstant],
) -> Vec<WeIrMaterialConstant> {
    constants
        .iter()
        .map(|constant| WeIrMaterialConstant {
            name: format!("{stage}.{}", constant.name),
            value_json: constant.value_json.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aggregate_flow_material_matches_the_typed_multiply_shader() {
        assert_eq!(
            flow_composite_shader(SceneBlendMode::Multiply),
            RIPPLE_FLOW_MULTIPLY_COMPOSITE_SHADER
        );
        assert_eq!(
            flow_composite_shader(SceneBlendMode::Alpha),
            RIPPLE_FLOW_COMPOSITE_SHADER
        );
    }
}
