//! Typed direct composite for a textureless flat layer with one rounded mask.

use super::{WeEffectPassContract, WeImageGraphContract};
use crate::core::SceneBlendMode;
use crate::engine::render_graph::{
    PassState, RenderGraph, RenderPassNode, RenderPassRole, RenderTargetRole, TextureBindingRole,
};

const DIRECT_SHADER: &str = "we/flat-rounded-mask-composite";
const HSL_SOURCE_SHADER: &str = "we/flat-rounded-hsl-source";

pub(super) fn is_compatible(contract: &WeImageGraphContract) -> bool {
    supports_direct_chain(contract)
        && if contract.final_scene_blend == SceneBlendMode::HslColor {
            contract.framebuffer_snapshot.is_some()
        } else {
            contract.framebuffer_snapshot.is_none()
        }
}

pub(super) fn supports_direct_chain(contract: &WeImageGraphContract) -> bool {
    contract.base_material_index.is_some()
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
    if contract.final_scene_blend == SceneBlendMode::HslColor {
        append_hsl_snapshot_composite(graph, contract);
        return;
    }
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

fn append_hsl_snapshot_composite(graph: &mut RenderGraph, contract: &WeImageGraphContract) {
    let effect = &contract.effect_passes[0];
    let snapshot = contract
        .framebuffer_snapshot
        .as_ref()
        .expect("HSL rounded composite requires a typed scene snapshot");
    graph.passes.push(RenderPassNode {
        id: 0,
        role: RenderPassRole::CopyTarget,
        object_index: Some(contract.object_index),
        material_index: None,
        pass_index: 0,
        shader: None,
        target: RenderTargetRole::FirstClassEffectTarget,
        target_name: Some(snapshot.target_name.clone()),
        target_extent: None,
        target_format: Some("rgba_backbuffer".to_owned()),
        bindings: vec![TextureBindingRole::GraphTarget {
            slot: snapshot.texture_slot,
            role: RenderTargetRole::SceneColor,
            name: None,
        }],
        state: PassState::default(),
    });
    graph.passes.push(RenderPassNode {
        id: 1,
        role: RenderPassRole::SceneComposite,
        object_index: Some(contract.object_index),
        material_index: effect.material_index,
        pass_index: effect.pass_index,
        shader: Some(HSL_SOURCE_SHADER.to_owned()),
        target: RenderTargetRole::SceneColor,
        target_name: None,
        target_extent: None,
        target_format: None,
        bindings: std::iter::once(TextureBindingRole::EffectTarget {
            slot: snapshot.texture_slot,
            name: snapshot.target_name.clone(),
        })
        .chain(
            effect
                .pass_constants
                .iter()
                .cloned()
                .map(|name| TextureBindingRole::PassConstant { name }),
        )
        .collect(),
        state: PassState {
            pipeline_blend: crate::engine::render_graph::PipelineBlendMode::Normal,
            scene_blend: SceneBlendMode::Normal,
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
        && (effect.binds.is_empty()
            || effect
                .binds
                .get(&0)
                .is_some_and(|source| is_previous(source)))
        && effect
            .binds
            .iter()
            .all(|(slot, source)| *slot == 0 && is_previous(source))
        && effect_combo_value(effect, "B_SQUARE", 1) == 0
        && effect_combo_value(effect, "C_ALPHA_ONLY", 1) == 0
        && effect_combo_value(effect, "SOFT", 0) == 1
}

fn effect_combo_value(effect: &WeEffectPassContract, name: &str, default: i64) -> i64 {
    effect.combos.get(name).copied().unwrap_or_else(|| {
        let prefix = format!("{}_", name.to_ascii_uppercase());
        effect
            .shader
            .as_deref()
            .into_iter()
            .flat_map(|shader| shader.split("__"))
            .find_map(|component| {
                component
                    .to_ascii_uppercase()
                    .strip_prefix(&prefix)
                    .and_then(|value| value.parse().ok())
            })
            .unwrap_or(default)
    })
}

fn is_previous(source: &str) -> bool {
    matches!(source, "previous" | "_previous" | "$previous")
}
