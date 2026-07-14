//! Typed two-stage lowering for authored-texture water-ripple then water-flow chains.

use super::{WeEffectPassContract, WeImageGraphContract};
use crate::core::SceneBlendMode;
use crate::engine::render_graph::{
    PassState, RenderGraph, RenderPassNode, RenderPassRole, RenderTargetRole, TextureBindingRole,
};

const RIPPLE_SOURCE_SHADER: &str = "we/image-ripple-source";
const FLOW_COMPOSITE_SHADER: &str = "we/image-ripple-flow-composite";
const FLOW_MULTIPLY_COMPOSITE_SHADER: &str = "we/image-ripple-flow-multiply-composite";

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
        && contract.ripple_flow_material_indices.is_some()
        && are_compatible_effect_passes(&contract.effect_passes)
}

pub(super) fn are_compatible_effect_passes(effects: &[WeEffectPassContract]) -> bool {
    effects.len() == 2 && compatible_ripple(&effects[0]) && compatible_flow(&effects[1])
}

pub(super) fn append_two_stage_composite(graph: &mut RenderGraph, contract: &WeImageGraphContract) {
    let ripple = &contract.effect_passes[0];
    let materials = contract
        .ripple_flow_material_indices
        .expect("compatible ripple/flow graph has typed materials");
    graph.passes.push(RenderPassNode {
        id: 0,
        role: RenderPassRole::EffectMaterial,
        object_index: Some(contract.object_index),
        material_index: Some(materials.ripple_source),
        pass_index: ripple.pass_index,
        shader: Some(RIPPLE_SOURCE_SHADER.to_owned()),
        target: RenderTargetRole::ImageLocalMain,
        target_name: None,
        target_extent: None,
        target_format: None,
        bindings: Vec::new(),
        state: PassState {
            pipeline_blend: super::base_pipeline_blend(contract),
            scene_blend: contract.final_scene_blend,
            ..PassState::default()
        },
    });
    graph.passes.push(RenderPassNode {
        id: 1,
        role: RenderPassRole::SceneComposite,
        object_index: Some(contract.object_index),
        material_index: Some(materials.flow_composite),
        pass_index: 1,
        shader: Some(flow_composite_shader(contract.final_scene_blend).to_owned()),
        target: RenderTargetRole::SceneColor,
        target_name: None,
        target_extent: None,
        target_format: None,
        bindings: vec![TextureBindingRole::PreviousGraphTarget { slot: 0 }],
        state: PassState {
            pipeline_blend: super::final_pipeline_blend(contract),
            scene_blend: contract.final_scene_blend,
            ..PassState::default()
        },
    });
}

fn flow_composite_shader(scene_blend: SceneBlendMode) -> &'static str {
    if scene_blend == SceneBlendMode::Multiply {
        FLOW_MULTIPLY_COMPOSITE_SHADER
    } else {
        FLOW_COMPOSITE_SHADER
    }
}

fn compatible_ripple(pass: &WeEffectPassContract) -> bool {
    compatible_previous_only_pass(pass, "waterripple", &[0, 2])
        && pass.binds.contains_key(&2)
        && combo_disabled(pass, "MASK")
        && combo_disabled(pass, "PERSPECTIVE")
        && combo_disabled(pass, "SPECULAR")
}

fn compatible_flow(pass: &WeEffectPassContract) -> bool {
    compatible_previous_only_pass(pass, "waterflow", &[0, 1, 2])
        && pass.binds.contains_key(&1)
        && pass.binds.contains_key(&2)
}

fn compatible_previous_only_pass(
    pass: &WeEffectPassContract,
    expected_shader: &str,
    allowed_slots: &[u32],
) -> bool {
    pass.command.is_none()
        && pass.source.is_none()
        && pass.target.is_none()
        && pass.material_index.is_some()
        && pass
            .shader
            .as_deref()
            .is_some_and(|shader| shader_basename(shader) == expected_shader)
        && pass
            .binds
            .get(&0)
            .is_some_and(|source| is_previous_source(source))
        && pass.binds.iter().all(|(slot, source)| {
            allowed_slots.contains(slot) && (*slot == 0 || !is_graph_resource(source))
        })
        && pass
            .material_blending
            .as_deref()
            .is_none_or(|blend| blend.eq_ignore_ascii_case("normal"))
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

    use super::super::WeRippleFlowMaterialIndices;
    use super::*;
    use crate::core::SceneBlendMode;

    #[test]
    fn compatible_chain_removes_base_and_terminal_copy_passes() {
        let contract = WeImageGraphContract {
            object_index: 4,
            base_material_index: Some(2),
            base_shader: Some("genericimage2".to_owned()),
            base_material_blending: Some("translucent".to_owned()),
            base_texture_slots: vec![0],
            base_pass_constants: Vec::new(),
            framebuffer_snapshot: None,
            final_scene_blend: SceneBlendMode::Alpha,
            effects_in_authored_texture_space: true,
            puppet_skinning_after_effects: false,
            waterwaves_uv_field_material_index: None,
            foliage_ripple_material: None,
            ripple_flow_material_indices: Some(WeRippleFlowMaterialIndices {
                ripple_source: 7,
                flow_composite: 8,
            }),
            final_effect_material: None,
            effect_passes: vec![ripple(), flow()],
        };

        let graph = super::super::we_image_graph(&contract);

        assert_eq!(graph.passes.len(), 2);
        assert_eq!(
            graph.passes[0].shader.as_deref(),
            Some(RIPPLE_SOURCE_SHADER)
        );
        assert_eq!(graph.passes[0].target, RenderTargetRole::ImageLocalMain);
        assert_eq!(
            graph.passes[1].shader.as_deref(),
            Some(FLOW_COMPOSITE_SHADER)
        );
        assert_eq!(graph.passes[1].target, RenderTargetRole::SceneColor);
        assert_eq!(graph.passes[1].material_index, Some(8));
    }

    #[test]
    fn missing_flow_phase_keeps_the_general_graph() {
        let mut effects = vec![ripple(), flow()];
        effects[1].binds.remove(&2);
        assert!(!are_compatible_effect_passes(&effects));
    }

    #[test]
    fn multiply_chain_selects_the_premultiplied_fixed_blend_shader() {
        let mut contract = WeImageGraphContract {
            object_index: 4,
            base_material_index: Some(2),
            base_shader: Some("genericimage2".to_owned()),
            base_material_blending: Some("translucent".to_owned()),
            base_texture_slots: vec![0],
            base_pass_constants: Vec::new(),
            framebuffer_snapshot: None,
            final_scene_blend: SceneBlendMode::Multiply,
            effects_in_authored_texture_space: true,
            puppet_skinning_after_effects: false,
            waterwaves_uv_field_material_index: None,
            foliage_ripple_material: None,
            ripple_flow_material_indices: Some(WeRippleFlowMaterialIndices {
                ripple_source: 7,
                flow_composite: 8,
            }),
            final_effect_material: None,
            effect_passes: vec![ripple(), flow()],
        };

        let graph = super::super::we_image_graph(&contract);
        assert_eq!(
            graph.passes[1].shader.as_deref(),
            Some(FLOW_MULTIPLY_COMPOSITE_SHADER)
        );

        contract.final_scene_blend = SceneBlendMode::Alpha;
        let graph = super::super::we_image_graph(&contract);
        assert_eq!(
            graph.passes[1].shader.as_deref(),
            Some(FLOW_COMPOSITE_SHADER)
        );
    }

    fn ripple() -> WeEffectPassContract {
        effect(
            "effects/waterripple__SLOTS_5",
            5,
            BTreeMap::from([(0, "previous".to_owned()), (2, "normal".to_owned())]),
        )
    }

    fn flow() -> WeEffectPassContract {
        effect(
            "effects/waterflow__SLOTS_7",
            6,
            BTreeMap::from([
                (0, "previous".to_owned()),
                (1, "flow".to_owned()),
                (2, "phase".to_owned()),
            ]),
        )
    }

    fn effect(
        shader: &str,
        material_index: usize,
        binds: BTreeMap<u32, String>,
    ) -> WeEffectPassContract {
        WeEffectPassContract {
            object_index: 4,
            material_index: Some(material_index),
            effect_file: format!("{shader}/effect.json"),
            pass_index: 0,
            command: None,
            shader: Some(shader.to_owned()),
            source: None,
            target: None,
            binds,
            pass_constants: Vec::new(),
            material_blending: Some("normal".to_owned()),
            depthtest: Some("disabled".to_owned()),
            depthwrite: Some("disabled".to_owned()),
            cullmode: Some("nocull".to_owned()),
            combos: BTreeMap::new(),
        }
    }
}
