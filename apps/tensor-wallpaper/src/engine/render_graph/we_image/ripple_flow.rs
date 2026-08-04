//! Typed authored pass preservation for terminal water-flow chains.

use super::{WeEffectPassContract, WeImageGraphContract};
use crate::core::SceneBlendMode;
use crate::engine::render_graph::{
    ColorWriteMask, PassState, PipelineBlendMode, RenderGraph, RenderPassDrawPrimitive,
    RenderPassEffectVisibility, RenderPassNode, RenderPassRole, RenderTargetRole,
    TextureBindingRole,
};

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
        && are_compatible_effect_passes(&contract.effect_passes)
}

pub(super) fn are_compatible_effect_passes(effects: &[WeEffectPassContract]) -> bool {
    match effects {
        [flow] => compatible_flow(flow),
        [ripple, flow] => compatible_ripple(ripple) && compatible_flow(flow),
        _ => false,
    }
}

pub(super) fn append_authored_terminal_flow_chain(
    graph: &mut RenderGraph,
    contract: &WeImageGraphContract,
) {
    let ripple = (contract.effect_passes.len() == 2).then(|| &contract.effect_passes[0]);
    let flow = contract
        .effect_passes
        .last()
        .expect("compatible terminal flow graph has a flow pass");
    graph.passes.push(RenderPassNode {
        id: 0,
        role: RenderPassRole::ObjectLocalSource,
        draw_primitive: RenderPassDrawPrimitive::ObjectMesh,
        object_index: Some(contract.object_index),
        material_index: contract.base_material_index,
        pass_index: 0,
        shader: contract.base_shader.clone(),
        target: RenderTargetRole::ImageLocalMain,
        target_name: None,
        target_extent: None,
        target_format: Some("rgba8".to_owned()),
        bindings: std::iter::once(TextureBindingRole::SourceTexture)
            .chain(
                contract
                    .base_texture_slots
                    .iter()
                    .copied()
                    .filter(|slot| *slot != 0)
                    .map(|slot| TextureBindingRole::TextureSlot { slot }),
            )
            .chain(
                contract
                    .base_pass_constants
                    .iter()
                    .cloned()
                    .map(|name| TextureBindingRole::PassConstant { name }),
            )
            .collect(),
        effect_visibility: RenderPassEffectVisibility::NONE,
        state: PassState {
            pipeline_blend: PipelineBlendMode::Normal,
            scene_blend: SceneBlendMode::Normal,
            ..PassState::default()
        },
    });
    if let Some(ripple) = ripple {
        let mut ripple_node = super::we_effect_pass_node(1, ripple, contract.final_scene_blend);
        ripple_node.role = RenderPassRole::ObjectLocalSource;
        ripple_node.draw_primitive = RenderPassDrawPrimitive::ObjectMesh;
        ripple_node.target = RenderTargetRole::ImageLocalSub;
        ripple_node.target_name = None;
        ripple_node.target_format = Some("rgba8".to_owned());
        ripple_node.state.pipeline_blend = PipelineBlendMode::Normal;
        ripple_node.state.scene_blend = SceneBlendMode::Normal;
        graph.passes.push(ripple_node);
    }

    let flow_id = graph.passes.len().min(u32::MAX as usize) as u32;
    let mut flow_node = super::we_effect_pass_node(flow_id, flow, contract.final_scene_blend);
    flow_node.role = RenderPassRole::SceneComposite;
    flow_node.draw_primitive = RenderPassDrawPrimitive::ObjectCompositeMesh;
    flow_node.target = RenderTargetRole::SceneColor;
    flow_node.target_name = None;
    flow_node.target_format = None;
    flow_node.state.pipeline_blend = super::final_pipeline_blend(contract);
    flow_node.state.color_write_mask = ColorWriteMask::Rgb;
    graph.passes.push(flow_node);
}

fn compatible_ripple(pass: &WeEffectPassContract) -> bool {
    pass.effect_binding_count == 1
        && compatible_previous_only_pass(pass, "waterripple", &[0, 2])
        && pass.binds.contains_key(&2)
        && combo_disabled(pass, "MASK")
        && combo_disabled(pass, "PERSPECTIVE")
        && combo_disabled(pass, "SPECULAR")
}

fn compatible_flow(pass: &WeEffectPassContract) -> bool {
    pass.effect_binding_count == 1
        && compatible_previous_only_pass(pass, "waterflow", &[0, 1, 2])
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

    use super::*;
    use crate::engine::render_graph::RenderPassEffectVisibilityPolicy;

    #[test]
    fn compatible_chain_preserves_three_authored_draws_and_two_local_targets() {
        let contract = WeImageGraphContract {
            object_index: 4,
            base_material_index: Some(2),
            base_shader: Some("genericimage2".to_owned()),
            base_material_blending: Some("translucent".to_owned()),
            base_texture_slots: vec![0],
            base_pass_constants: Vec::new(),
            color_blend_mode: 0,
            framebuffer_snapshot: None,
            final_scene_blend: SceneBlendMode::Alpha,
            static_black_output: false,
            effects_in_authored_texture_space: true,
            puppet_skinning_after_effects: false,
            waterwaves_uv_field_material_index: None,
            waterwaves_direct_material: None,
            foliage_ripple_material: None,
            final_effect_material: None,
            effect_passes: vec![ripple(), flow()],
        };

        let graph = super::super::we_image_graph(&contract);

        assert_eq!(graph.passes.len(), 3);
        assert_eq!(graph.passes[0].shader.as_deref(), Some("genericimage2"));
        assert_eq!(graph.passes[0].role, RenderPassRole::ObjectLocalSource);
        assert_eq!(
            graph.passes[0].draw_primitive,
            RenderPassDrawPrimitive::ObjectMesh
        );
        assert_eq!(graph.passes[0].target, RenderTargetRole::ImageLocalMain);
        assert_eq!(
            graph.passes[0].state.pipeline_blend,
            PipelineBlendMode::Normal
        );
        assert_eq!(
            graph.passes[1].shader.as_deref(),
            Some("effects/waterripple__SLOTS_5")
        );
        assert_eq!(graph.passes[1].role, RenderPassRole::ObjectLocalSource);
        assert_eq!(
            graph.passes[1].draw_primitive,
            RenderPassDrawPrimitive::ObjectMesh
        );
        assert_eq!(graph.passes[1].target, RenderTargetRole::ImageLocalSub);
        assert_eq!(graph.passes[1].material_index, Some(5));
        assert_eq!(
            graph.passes[2].shader.as_deref(),
            Some("effects/waterflow__SLOTS_7")
        );
        assert_eq!(graph.passes[2].role, RenderPassRole::SceneComposite);
        assert_eq!(
            graph.passes[2].draw_primitive,
            RenderPassDrawPrimitive::ObjectCompositeMesh
        );
        assert_eq!(graph.passes[2].target, RenderTargetRole::SceneColor);
        assert_eq!(graph.passes[2].material_index, Some(6));
        assert_eq!(graph.passes[2].state.color_write_mask, ColorWriteMask::Rgb);
        assert_eq!(
            graph.passes[1].effect_visibility.policy,
            RenderPassEffectVisibilityPolicy::Passthrough
        );
        assert_eq!(graph.passes[1].effect_visibility.binding_start, 5);
        assert_eq!(graph.passes[1].effect_visibility.binding_count, 1);
        assert_eq!(
            graph.passes[2].effect_visibility.policy,
            RenderPassEffectVisibilityPolicy::Passthrough
        );
        assert_eq!(graph.passes[2].effect_visibility.binding_start, 6);
        assert_eq!(graph.passes[2].effect_visibility.binding_count, 1);
    }

    #[test]
    fn lone_flow_preserves_local_source_and_authored_scene_composite() {
        let contract = WeImageGraphContract {
            object_index: 4,
            base_material_index: Some(2),
            base_shader: Some("genericimage4".to_owned()),
            base_material_blending: Some("translucent".to_owned()),
            base_texture_slots: vec![0],
            base_pass_constants: Vec::new(),
            color_blend_mode: 0,
            framebuffer_snapshot: None,
            final_scene_blend: SceneBlendMode::Alpha,
            static_black_output: false,
            effects_in_authored_texture_space: true,
            puppet_skinning_after_effects: false,
            waterwaves_uv_field_material_index: None,
            waterwaves_direct_material: None,
            foliage_ripple_material: None,
            final_effect_material: None,
            effect_passes: vec![flow()],
        };

        let graph = super::super::we_image_graph(&contract);

        assert_eq!(graph.passes.len(), 2);
        assert_eq!(graph.passes[0].role, RenderPassRole::ObjectLocalSource);
        assert_eq!(graph.passes[0].shader.as_deref(), Some("genericimage4"));
        assert_eq!(graph.passes[0].target, RenderTargetRole::ImageLocalMain);
        assert_eq!(
            graph.passes[0].draw_primitive,
            RenderPassDrawPrimitive::ObjectMesh
        );
        assert_eq!(graph.passes[1].role, RenderPassRole::SceneComposite);
        assert_eq!(
            graph.passes[1].shader.as_deref(),
            Some("effects/waterflow__SLOTS_7")
        );
        assert_eq!(graph.passes[1].target, RenderTargetRole::SceneColor);
        assert_eq!(
            graph.passes[1].draw_primitive,
            RenderPassDrawPrimitive::ObjectCompositeMesh
        );
        assert_eq!(graph.passes[1].state.color_write_mask, ColorWriteMask::Rgb);
        assert_eq!(
            graph.passes[1].effect_visibility.policy,
            RenderPassEffectVisibilityPolicy::Passthrough
        );
    }

    #[test]
    fn missing_flow_phase_keeps_the_general_graph() {
        let mut effects = vec![ripple(), flow()];
        effects[1].binds.remove(&2);
        assert!(!are_compatible_effect_passes(&effects));
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
            effect_binding_start: material_index as u32,
            effect_binding_count: 1,
            runtime_visibility: true,
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
