//! Typed direct composite for a compatible foliage-sway then water-ripple chain.

use super::{WeEffectPassContract, WeImageGraphContract};
use crate::engine::render_graph::{
    PassState, RenderGraph, RenderPassNode, RenderPassRole, RenderTargetRole,
};

#[cfg(test)]
const DIRECT_SHADER: &str = "we/image-foliage-ripple-composite";
#[cfg(test)]
const SCREEN_DIRECT_SHADER: &str = "we/image-foliage-ripple-screen-composite";

pub(super) fn is_compatible(contract: &WeImageGraphContract) -> bool {
    contract.effects_in_authored_texture_space
        && !contract.puppet_skinning_after_effects
        && contract.framebuffer_snapshot.is_none()
        && contract.base_material_index.is_some()
        && contract
            .base_shader
            .as_deref()
            .is_some_and(is_generic_image_shader)
        && contract.base_texture_slots.as_slice() == [0]
        && contract.foliage_ripple_material.is_some()
        && are_compatible_effect_passes(&contract.effect_passes)
        && super::contiguous_effect_range(&contract.effect_passes).is_some()
}

pub(super) fn are_compatible_effect_passes(effects: &[WeEffectPassContract]) -> bool {
    effects.len() == 2
        && effects
            .iter()
            .all(|effect| effect.effect_binding_count == 1)
        && compatible_previous_only_pass(&effects[0])
        && compatible_previous_only_pass(&effects[1])
        && effects[0]
            .shader
            .as_deref()
            .is_some_and(|shader| shader_basename(shader) == "foliagesway")
        && effects[1]
            .shader
            .as_deref()
            .is_some_and(|shader| shader_basename(shader) == "waterripple")
        && combo_disabled(&effects[0], "MODE")
        && combo_disabled(&effects[0], "MASK")
        && combo_disabled(&effects[1], "MASK")
        && combo_disabled(&effects[1], "PERSPECTIVE")
        && combo_disabled(&effects[1], "SPECULAR")
}

pub(super) fn append_direct_composite(graph: &mut RenderGraph, contract: &WeImageGraphContract) {
    let material = contract
        .foliage_ripple_material
        .as_ref()
        .expect("compatible foliage/ripple graph requires its typed material");
    let effect_visibility = super::contiguous_material_stage_visibility(&contract.effect_passes)
        .expect("typed foliage/ripple graph requires contiguous effect bindings");
    graph.passes.push(RenderPassNode {
        id: 0,
        role: RenderPassRole::SceneComposite,
        object_index: Some(contract.object_index),
        material_index: Some(material.material_index),
        pass_index: 0,
        shader: Some(material.shader.clone()),
        target: RenderTargetRole::SceneColor,
        target_name: None,
        target_extent: None,
        target_format: None,
        bindings: Vec::new(),
        effect_visibility,
        state: PassState {
            pipeline_blend: super::final_pipeline_blend(contract),
            scene_blend: contract.final_scene_blend,
            ..PassState::default()
        },
    });
}

fn compatible_previous_only_pass(pass: &WeEffectPassContract) -> bool {
    pass.command.is_none()
        && pass.source.is_none()
        && pass.target.is_none()
        && pass.material_index.is_some()
        && pass
            .binds
            .get(&0)
            .is_some_and(|source| matches!(source.as_str(), "previous" | "_previous" | "$previous"))
        && pass.binds.iter().all(|(slot, source)| {
            *slot == 0
                || (*slot == 2
                    && !matches!(
                        source.as_str(),
                        "previous" | "_previous" | "$previous" | "source"
                    )
                    && !source.starts_with("fbo_")
                    && !source.starts_with("_rt_")
                    && !source.starts_with("_alias_"))
        })
}

fn is_generic_image_shader(shader: &str) -> bool {
    matches!(
        shader_basename(shader).as_str(),
        "genericimage2" | "genericimage4"
    )
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

fn combo_disabled(pass: &WeEffectPassContract, name: &str) -> bool {
    !pass
        .combos
        .iter()
        .any(|(candidate, value)| candidate.eq_ignore_ascii_case(name) && *value != 0)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::super::WeFoliageRippleMaterial;
    use super::*;
    use crate::core::SceneBlendMode;
    use crate::engine::render_graph::RenderPassEffectVisibilityPolicy;

    #[test]
    fn compatible_chain_lowers_to_one_direct_scene_composite() {
        let contract = WeImageGraphContract {
            object_index: 5,
            base_material_index: Some(7),
            base_shader: Some("genericimage4".to_owned()),
            base_material_blending: Some("translucent".to_owned()),
            base_texture_slots: vec![0],
            base_pass_constants: Vec::new(),
            framebuffer_snapshot: None,
            final_scene_blend: SceneBlendMode::Alpha,
            effects_in_authored_texture_space: true,
            puppet_skinning_after_effects: false,
            waterwaves_uv_field_material_index: None,
            waterwaves_direct_material: None,
            foliage_ripple_material: Some(WeFoliageRippleMaterial {
                material_index: 12,
                shader: DIRECT_SHADER.to_owned(),
            }),
            ripple_flow_material_indices: None,
            final_effect_material: None,
            effect_passes: vec![effect("foliagesway", 10), effect("waterripple", 11)],
        };

        let graph = super::super::we_image_graph(&contract);

        assert_eq!(graph.passes.len(), 1);
        assert_eq!(graph.passes[0].role, RenderPassRole::SceneComposite);
        assert_eq!(graph.passes[0].shader.as_deref(), Some(DIRECT_SHADER));
        assert_eq!(graph.passes[0].material_index, Some(12));
        assert_eq!(
            graph.passes[0].effect_visibility.policy,
            RenderPassEffectVisibilityPolicy::MaterialStages
        );
        assert_eq!(graph.passes[0].effect_visibility.binding_start, 10);
        assert_eq!(graph.passes[0].effect_visibility.binding_count, 2);
        assert!(graph.target_specs.is_empty());
    }

    #[test]
    fn screen_chain_selects_the_premultiplied_standard_blend_shader() {
        let mut contract = WeImageGraphContract {
            object_index: 5,
            base_material_index: Some(7),
            base_shader: Some("genericimage4".to_owned()),
            base_material_blending: Some("translucent".to_owned()),
            base_texture_slots: vec![0],
            base_pass_constants: Vec::new(),
            framebuffer_snapshot: None,
            final_scene_blend: SceneBlendMode::Screen,
            effects_in_authored_texture_space: true,
            puppet_skinning_after_effects: false,
            waterwaves_uv_field_material_index: None,
            waterwaves_direct_material: None,
            foliage_ripple_material: Some(WeFoliageRippleMaterial {
                material_index: 12,
                shader: SCREEN_DIRECT_SHADER.to_owned(),
            }),
            ripple_flow_material_indices: None,
            final_effect_material: None,
            effect_passes: vec![effect("foliagesway", 10), effect("waterripple", 11)],
        };

        let graph = super::super::we_image_graph(&contract);
        assert_eq!(
            graph.passes[0].shader.as_deref(),
            Some(SCREEN_DIRECT_SHADER)
        );

        contract.final_scene_blend = SceneBlendMode::Alpha;
        contract
            .foliage_ripple_material
            .as_mut()
            .expect("typed foliage material")
            .shader = DIRECT_SHADER.to_owned();
        let graph = super::super::we_image_graph(&contract);
        assert_eq!(graph.passes[0].shader.as_deref(), Some(DIRECT_SHADER));
    }

    fn effect(shader: &str, material_index: usize) -> WeEffectPassContract {
        WeEffectPassContract {
            object_index: 5,
            effect_binding_start: material_index as u32,
            effect_binding_count: 1,
            material_index: Some(material_index),
            effect_file: format!("effects/{shader}/effect.json"),
            pass_index: 0,
            command: None,
            shader: Some(format!("effects/{shader}__SLOTS_5")),
            source: None,
            target: None,
            binds: BTreeMap::from([(0, "previous".to_owned())]),
            pass_constants: Vec::new(),
            material_blending: Some("normal".to_owned()),
            depthtest: Some("disabled".to_owned()),
            depthwrite: Some("disabled".to_owned()),
            cullmode: Some("nocull".to_owned()),
            combos: BTreeMap::new(),
        }
    }
}
