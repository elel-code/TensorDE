//! Typed direct composite for a textureless flat layer with one rounded mask.

use super::{WeEffectPassContract, WeImageGraphContract};
use crate::engine::render_graph::{
    PassState, RenderGraph, RenderPassNode, RenderPassRole, RenderTargetRole, TextureBindingRole,
};

const DIRECT_SHADER: &str = "we/flat-rounded-mask-composite";

pub(super) fn is_compatible(contract: &WeImageGraphContract) -> bool {
    contract.framebuffer_snapshot.is_none()
        && contract.base_material_index.is_some()
        && contract
            .base_shader
            .as_deref()
            .is_some_and(base_shader_is_flat)
        && contract.base_texture_slots.is_empty()
        && contract.base_pass_constants.is_empty()
        && contract.effect_passes.len() == 1
        && contract
            .effect_passes
            .first()
            .is_some_and(compatible_effect)
}

pub(super) fn append_direct_composite(graph: &mut RenderGraph, contract: &WeImageGraphContract) {
    let effect = &contract.effect_passes[0];
    graph.passes.push(RenderPassNode {
        id: 0,
        role: RenderPassRole::SceneComposite,
        object_index: Some(contract.object_index),
        material_index: effect.material_index,
        pass_index: effect.pass_index,
        shader: Some(DIRECT_SHADER.to_owned()),
        target: RenderTargetRole::SceneColor,
        target_name: None,
        target_extent: None,
        target_format: None,
        bindings: effect
            .pass_constants
            .iter()
            .cloned()
            .map(|name| TextureBindingRole::PassConstant { name })
            .collect(),
        state: PassState {
            pipeline_blend: super::final_pipeline_blend(contract),
            scene_blend: contract.final_scene_blend,
            ..PassState::default()
        },
    });
}

fn base_shader_is_flat(shader: &str) -> bool {
    shader
        .strip_prefix("we/")
        .unwrap_or(shader)
        .split("__")
        .next()
        .is_some_and(|shader| shader.eq_ignore_ascii_case("flat"))
}

fn compatible_effect(effect: &WeEffectPassContract) -> bool {
    effect.command.is_none()
        && effect.source.is_none()
        && effect.target.is_none()
        && effect.material_index.is_some()
        && effect.shader.as_deref().is_some_and(|shader| {
            shader
                .split("__")
                .next()
                .is_some_and(|shader| shader.ends_with("/rounded_mask"))
        })
        && effect
            .binds
            .get(&0)
            .is_some_and(|source| is_previous(source))
        && effect
            .binds
            .iter()
            .all(|(slot, source)| *slot == 0 && is_previous(source))
        && effect.combos.get("B_SQUARE") == Some(&0)
        && effect.combos.get("C_ALPHA_ONLY") == Some(&0)
        && effect.combos.get("SOFT") == Some(&1)
}

fn is_previous(source: &str) -> bool {
    matches!(source, "previous" | "_previous" | "$previous")
}
