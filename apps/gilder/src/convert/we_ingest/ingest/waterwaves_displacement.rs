//! Converter-owned material aggregation for typed waterwaves UV fields.

use crate::convert::we_ingest::ir::{WeIrMaterial, WeIrMaterialConstant, WeIrMaterialTexture};
use crate::core::SceneBlendMode;
use crate::engine::render_graph::{
    WeEffectPassContract, WeWaterWavesDirectMaterial,
    we_effect_passes_form_waterwaves_displacement_chain,
};
use crate::engine::scene::{SceneCullMode, SceneDepthTest, ScenePipelineBlend};

use super::WeIrBuilder;

pub(super) const WATERWAVES_UV_FIELD_SHADER: &str = "we/waterwaves-uv-field";
const DISABLE_WATERWAVES_AGGREGATION_ENV: &str = "GILDER_CONVERT_DISABLE_WATERWAVES_AGGREGATION";
const WATERWAVES_UV_FIELD_ENV: &str = "GILDER_CONVERT_WATERWAVES_UV_FIELD";
const IMAGE_DIRECT_SHADER: &str = "we/image-waterwaves-direct";
const IMAGE_MULTIPLY_DIRECT_SHADER: &str = "we/image-waterwaves-multiply-direct";
const PUPPET_DIRECT_SHADER: &str = "we/puppet-waterwaves-direct";
const EFFECT_RUN_DIRECT_SHADER: &str = "we/effect-waterwaves-direct";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct WaterWavesDisplacementMaterials {
    pub uv_field: Option<usize>,
    pub direct: Option<WeWaterWavesDirectMaterial>,
}

#[derive(Debug, Clone)]
struct WaterWavesStageMaterial {
    mask: Option<WeIrMaterialTexture>,
    constants: Vec<WeIrMaterialConstant>,
    dual_waves: bool,
    masked: bool,
}

// Keep every authored chain decision explicit at this cold lowering boundary.
#[allow(clippy::too_many_arguments)]
pub(super) fn create_waterwaves_displacement_materials(
    builder: &mut WeIrBuilder,
    authored_texture_space: bool,
    base_material_index: usize,
    final_scene_blend: SceneBlendMode,
    object_is_puppet: bool,
    static_black_output: bool,
    puppet_group_visual_required: bool,
    effects: &[WeEffectPassContract],
) -> WaterWavesDisplacementMaterials {
    if std::env::var_os(DISABLE_WATERWAVES_AGGREGATION_ENV).is_some() {
        return WaterWavesDisplacementMaterials::default();
    }
    if object_is_puppet
        || !authored_texture_space
        || !we_effect_passes_form_waterwaves_displacement_chain(effects)
        || compatible_effect_binding_count(effects).is_none()
    {
        return WaterWavesDisplacementMaterials::default();
    }
    let direct_shader = std::env::var_os(WATERWAVES_UV_FIELD_ENV)
        .is_none()
        .then(|| {
            if object_is_puppet {
                PUPPET_DIRECT_SHADER
            } else if final_scene_blend == SceneBlendMode::Multiply {
                IMAGE_MULTIPLY_DIRECT_SHADER
            } else {
                IMAGE_DIRECT_SHADER
            }
        });
    let material = create_aggregated_material(
        builder,
        base_material_index,
        effects,
        direct_shader,
        static_black_output,
        true,
        puppet_group_visual_required,
    );
    if direct_shader.is_some() {
        WaterWavesDisplacementMaterials {
            direct: material.map(|(material_index, shader)| WeWaterWavesDirectMaterial {
                material_index,
                shader,
                group_visual_composite: puppet_group_visual_required,
            }),
            ..WaterWavesDisplacementMaterials::default()
        }
    } else {
        WaterWavesDisplacementMaterials {
            uv_field: material.map(|(material_index, _)| material_index),
            ..WaterWavesDisplacementMaterials::default()
        }
    }
}

pub(super) fn aggregate_waterwaves_effect_runs(
    builder: &mut WeIrBuilder,
    effects: &[WeEffectPassContract],
) -> Vec<WeEffectPassContract> {
    if std::env::var_os(DISABLE_WATERWAVES_AGGREGATION_ENV).is_some() {
        return effects.to_vec();
    }
    let mut aggregated = Vec::with_capacity(effects.len());
    let mut cursor = 0;
    while cursor < effects.len() {
        let run_count = compatible_run_count(effects, cursor);
        let Some(run_count) = run_count else {
            aggregated.push(effects[cursor].clone());
            cursor += 1;
            continue;
        };
        let run = &effects[cursor..cursor + run_count];
        let Some(effect_binding_count) = compatible_effect_binding_count(run) else {
            aggregated.push(effects[cursor].clone());
            cursor += 1;
            continue;
        };
        let Some(source_material_index) = run[0].material_index else {
            aggregated.push(effects[cursor].clone());
            cursor += 1;
            continue;
        };
        let Some((material_index, shader)) = create_aggregated_material(
            builder,
            source_material_index,
            run,
            Some(EFFECT_RUN_DIRECT_SHADER),
            false,
            false,
            false,
        ) else {
            aggregated.push(effects[cursor].clone());
            cursor += 1;
            continue;
        };
        let mut effect = run[0].clone();
        effect.effect_binding_count = effect_binding_count;
        effect.material_index = Some(material_index);
        effect.effect_file = "gilder/typed/waterwaves-effect-run".to_owned();
        effect.shader = Some(shader);
        effect.source = None;
        effect.target = None;
        effect.command = None;
        effect.pass_constants.clear();
        effect.combos.clear();
        effect.binds.clear();
        effect.binds.insert(0, "previous".to_owned());
        for (stage, stage_effect) in run.iter().enumerate() {
            if let Some(mask) = stage_effect.binds.get(&1) {
                effect.binds.insert(stage as u32 + 1, mask.clone());
            }
        }
        aggregated.push(effect);
        cursor += run_count;
    }
    aggregated
}

fn compatible_run_count(effects: &[WeEffectPassContract], cursor: usize) -> Option<usize> {
    let max_count = effects.len().saturating_sub(cursor).min(9);
    (2..=max_count).rev().find(|count| {
        let run = &effects[cursor..cursor + count];
        we_effect_passes_form_waterwaves_displacement_chain(run)
            && compatible_effect_binding_count(run).is_some()
    })
}

fn compatible_effect_binding_count(effects: &[WeEffectPassContract]) -> Option<u32> {
    let first = effects.first()?;
    if effects.iter().all(|effect| !effect.runtime_visibility) {
        let mut previous = None;
        for effect in effects {
            if effect.effect_binding_count != 1
                || previous.is_some_and(|previous| effect.effect_binding_start <= previous)
            {
                return None;
            }
            previous = Some(effect.effect_binding_start);
        }
        return u32::try_from(effects.len()).ok();
    }
    if effects.iter().any(|effect| !effect.runtime_visibility) {
        return None;
    }
    let mut next = first.effect_binding_start;
    for effect in effects {
        if effect.effect_binding_start != next || effect.effect_binding_count == 0 {
            return None;
        }
        next = next.checked_add(effect.effect_binding_count)?;
    }
    Some(next - first.effect_binding_start)
}

fn create_aggregated_material(
    builder: &mut WeIrBuilder,
    base_material_index: usize,
    effects: &[WeEffectPassContract],
    direct_shader: Option<&str>,
    static_black_output: bool,
    include_source_material: bool,
    defer_source_visual: bool,
) -> Option<(usize, String)> {
    let mut stages = Vec::with_capacity(effects.len());
    let mut template = None;
    let base_material = builder.materials.get(base_material_index)?.clone();
    let base_pass = builder
        .material_passes
        .get(base_material.pass_start as usize)?
        .clone();
    let base_textures = builder
        .material_textures
        .get(
            base_pass.texture_start as usize
                ..base_pass
                    .texture_start
                    .saturating_add(base_pass.texture_count) as usize,
        )?
        .to_vec();
    let base_constants = builder
        .material_constants
        .get(
            base_pass.constant_start as usize
                ..base_pass
                    .constant_start
                    .saturating_add(base_pass.constant_count) as usize,
        )?
        .to_vec();
    let mut resource =
        (direct_shader.is_some() && include_source_material).then_some(base_material.resource);
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
    if direct_shader.is_some() && include_source_material {
        builder.material_textures.extend(
            base_textures
                .into_iter()
                .filter(|texture| texture.slot == 0),
        );
        if !defer_source_visual {
            builder.material_constants.extend(base_constants);
        }
    }
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
    pass.shader_key = direct_shader.map_or_else(
        || WATERWAVES_UV_FIELD_SHADER.to_owned(),
        |shader| direct_shader_key(shader, &stages, static_black_output),
    );
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
    let shader = pass.shader_key.clone();
    let pass_start = builder.material_passes.len() as u32;
    builder.material_passes.push(pass);
    builder.materials.push(WeIrMaterial {
        handle,
        resource: resource?,
        pass_start,
        pass_count: 1,
    });
    Some((handle as usize, shader))
}

fn direct_shader_key(
    shader: &str,
    stages: &[WaterWavesStageMaterial],
    static_black_output: bool,
) -> String {
    let static_black = if static_black_output {
        "__STATIC_BLACK_1"
    } else {
        ""
    };
    format!("{shader}__STAGES_{}{static_black}", stages.len())
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    #[test]
    fn direct_shader_key_records_the_typed_stage_count() {
        assert_eq!(
            direct_shader_key(
                IMAGE_DIRECT_SHADER,
                &[stage(&[], false), stage(&[], false)],
                false,
            ),
            "we/image-waterwaves-direct__STAGES_2",
        );
    }

    #[test]
    fn direct_shader_key_records_static_black_output_contract() {
        assert_eq!(
            direct_shader_key(
                PUPPET_DIRECT_SHADER,
                &[stage(&[], false), stage(&[], false)],
                true,
            ),
            "we/puppet-waterwaves-direct__STAGES_2__STATIC_BLACK_1",
        );
    }

    #[test]
    fn compatible_run_count_finds_waterwaves_inside_a_mixed_effect_chain() {
        let mut foliage = effect_pass(0, "effects/foliagesway");
        foliage.binds.insert(0, "previous".to_owned());
        let effects = vec![
            foliage,
            effect_pass(1, "effects/waterwaves__SLOTS_1"),
            effect_pass(2, "effects/waterwaves__SLOTS_3"),
            effect_pass(3, "effects/waterwaves__SLOTS_1"),
            effect_pass(4, "effects/blend__SLOTS_3__BLENDMODE_0"),
        ];

        assert_eq!(compatible_run_count(&effects, 0), None);
        assert_eq!(compatible_run_count(&effects, 1), Some(3));
        assert_eq!(compatible_run_count(&effects, 2), Some(2));
        assert_eq!(compatible_run_count(&effects, 3), None);
    }

    #[test]
    fn compatible_run_count_keeps_nine_stage_chain_in_one_program() {
        let effects = (0..9)
            .map(|binding| effect_pass(binding, "effects/waterwaves__SLOTS_1"))
            .collect::<Vec<_>>();
        assert_eq!(compatible_run_count(&effects, 0), Some(9));
    }

    fn effect_pass(effect_binding_start: u32, shader: &str) -> WeEffectPassContract {
        WeEffectPassContract {
            object_index: 0,
            effect_binding_start,
            effect_binding_count: 1,
            runtime_visibility: true,
            material_index: Some(0),
            effect_file: "effects/waterwaves/effect.json".to_owned(),
            pass_index: 0,
            command: None,
            shader: Some(shader.to_owned()),
            source: None,
            target: None,
            binds: BTreeMap::from([(0, "previous".to_owned())]),
            pass_constants: Vec::new(),
            material_blending: Some("normal".to_owned()),
            depthtest: None,
            depthwrite: None,
            cullmode: None,
            combos: BTreeMap::new(),
        }
    }

    fn stage(constants: &[(&str, &str)], dual_waves: bool) -> WaterWavesStageMaterial {
        WaterWavesStageMaterial {
            mask: None,
            constants: constants
                .iter()
                .map(|(name, value)| WeIrMaterialConstant {
                    name: (*name).to_owned(),
                    value_json: (*value).to_owned(),
                })
                .collect(),
            dual_waves,
            masked: false,
        }
    }
}
