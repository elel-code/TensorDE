//! Converter-owned material aggregation for typed waterwaves UV fields.

use crate::convert::we_ingest::ir::{WeIrMaterial, WeIrMaterialConstant, WeIrMaterialTexture};
use crate::engine::render_graph::{
    WeEffectPassContract, we_effect_passes_form_waterwaves_displacement_chain,
};
use crate::engine::scene::{SceneCullMode, SceneDepthTest, ScenePipelineBlend};

use super::WeIrBuilder;

pub(super) const WATERWAVES_UV_FIELD_SHADER: &str = "we/waterwaves-uv-field";

#[derive(Debug, Clone)]
struct WaterWavesStageMaterial {
    mask: Option<WeIrMaterialTexture>,
    constants: Vec<WeIrMaterialConstant>,
    dual_waves: bool,
    masked: bool,
}

pub(super) fn create_waterwaves_uv_field_material(
    builder: &mut WeIrBuilder,
    authored_texture_space: bool,
    effects: &[WeEffectPassContract],
) -> Option<usize> {
    if !authored_texture_space || !we_effect_passes_form_waterwaves_displacement_chain(effects) {
        return None;
    }

    let mut stages = Vec::with_capacity(effects.len());
    let mut template = None;
    let mut resource = None;
    for effect in effects {
        let material = builder.materials.get(effect.material_index?)?;
        let pass = builder
            .material_passes
            .get(material.pass_start as usize)?
            .clone();
        template.get_or_insert_with(|| pass.clone());
        resource.get_or_insert(material.resource);
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
        stages.push(WaterWavesStageMaterial {
            mask: textures.into_iter().find(|texture| texture.slot == 1),
            constants,
            dual_waves: combo_enabled(effect, "DUALWAVES"),
            masked: effect.binds.contains_key(&1),
        });
    }

    let handle = builder.materials.len() as u32;
    let texture_start = builder.material_textures.len() as u32;
    let constant_start = builder.material_constants.len() as u32;
    for (stage_index, stage) in stages.iter().enumerate() {
        if let Some(mask) = &stage.mask {
            let mut mask = mask.clone();
            mask.slot = stage_index as u32 + 1;
            builder.material_textures.push(mask);
        }
        for constant in &stage.constants {
            builder.material_constants.push(WeIrMaterialConstant {
                name: stage_parameter_name(stage_index, &constant.name),
                value_json: constant.value_json.clone(),
            });
        }
        builder.material_constants.push(WeIrMaterialConstant {
            name: stage_parameter_name(stage_index, "dualwaves"),
            value_json: u8::from(stage.dual_waves).to_string(),
        });
        builder.material_constants.push(WeIrMaterialConstant {
            name: stage_parameter_name(stage_index, "mask"),
            value_json: u8::from(stage.masked).to_string(),
        });
    }
    builder.material_constants.push(WeIrMaterialConstant {
        name: "waterwaves.stage_count".to_owned(),
        value_json: stages.len().to_string(),
    });

    let mut pass = template?;
    pass.material = handle;
    pass.shader_key = WATERWAVES_UV_FIELD_SHADER.to_owned();
    pass.target.clear();
    pass.texture_start = texture_start;
    pass.texture_count = builder.material_textures.len() as u32 - texture_start;
    pass.constant_start = constant_start;
    pass.constant_count = builder.material_constants.len() as u32 - constant_start;
    pass.pipeline_blend = ScenePipelineBlend::Normal;
    pass.depth_test = SceneDepthTest::Disabled;
    pass.depth_write = false;
    pass.cull_mode = SceneCullMode::None;
    pass.clear_target = false;
    let pass_start = builder.material_passes.len() as u32;
    builder.material_passes.push(pass);
    builder.materials.push(WeIrMaterial {
        handle,
        resource: resource?,
        pass_start,
        pass_count: 1,
    });
    Some(handle as usize)
}

fn combo_enabled(effect: &WeEffectPassContract, name: &str) -> bool {
    effect
        .combos
        .iter()
        .find(|(candidate, _)| candidate.eq_ignore_ascii_case(name))
        .is_some_and(|(_, value)| *value != 0)
        || effect.shader.as_deref().is_some_and(|shader| {
            let prefix = format!("{}_", name.to_ascii_uppercase());
            shader.split("__").any(|part| {
                part.to_ascii_uppercase()
                    .strip_prefix(&prefix)
                    .and_then(|value| value.parse::<i64>().ok())
                    .is_some_and(|value| value != 0)
            })
        })
}

fn stage_parameter_name(stage: usize, name: &str) -> String {
    format!("waterwaves.{stage}.{name}")
}
