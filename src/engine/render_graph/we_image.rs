use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::core::SceneBlendMode;

use super::binding::TextureBindingRole;
use super::graph::{RenderGraph, UnsupportedGraphBoundary};
use super::pass::{RenderPassNode, RenderPassRole};
use super::state::{CullMode, DepthTestMode, PassState, PipelineBlendMode, ShaderBlendMode};
use super::target::RenderTargetRole;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeEffectPassContract {
    pub object_index: usize,
    pub effect_file: String,
    pub pass_index: u32,
    pub shader: Option<String>,
    pub target: Option<String>,
    pub binds: BTreeMap<u32, String>,
    pub material_blending: Option<String>,
    pub depthtest: Option<String>,
    pub depthwrite: Option<String>,
    pub cullmode: Option<String>,
    pub combos: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeImageGraphContract {
    pub object_index: usize,
    pub base_shader: Option<String>,
    pub base_texture_slots: Vec<u32>,
    pub final_scene_blend: SceneBlendMode,
    pub effect_passes: Vec<WeEffectPassContract>,
}

pub fn we_image_graph(contract: &WeImageGraphContract) -> RenderGraph {
    let mut graph = RenderGraph::default();
    let has_effects = !contract.effect_passes.is_empty();
    graph.passes.push(RenderPassNode {
        id: 0,
        role: RenderPassRole::BaseMaterial,
        object_index: Some(contract.object_index),
        pass_index: 0,
        shader: contract.base_shader.clone(),
        target: if has_effects {
            RenderTargetRole::ImageLocalMain
        } else {
            RenderTargetRole::SceneColor
        },
        target_name: None,
        target_extent: None,
        bindings: std::iter::once(TextureBindingRole::SourceTexture)
            .chain(
                contract
                    .base_texture_slots
                    .iter()
                    .copied()
                    .map(|slot| TextureBindingRole::TextureSlot { slot }),
            )
            .collect(),
        state: PassState {
            scene_blend: if has_effects {
                SceneBlendMode::Normal
            } else {
                contract.final_scene_blend
            },
            ..PassState::default()
        },
    });
    for (index, effect) in contract.effect_passes.iter().enumerate() {
        let pass_id = (index + 1).min(u32::MAX as usize) as u32;
        let mut node = we_effect_pass_node(pass_id, effect, contract.final_scene_blend);
        if effect.target.is_none() && index + 1 == contract.effect_passes.len() {
            node.target = RenderTargetRole::SceneColor;
        } else if index + 1 < contract.effect_passes.len()
            && node.target == RenderTargetRole::ImageLocalMain
        {
            node.target = if index % 2 == 0 {
                RenderTargetRole::ImageLocalSub
            } else {
                RenderTargetRole::ImageLocalMain
            };
        }
        if node.bindings.is_empty() {
            node.bindings.push(TextureBindingRole::PreviousGraphTarget);
        }
        if node.shader.is_none() {
            graph.unsupported.push(UnsupportedGraphBoundary {
                object_index: Some(contract.object_index),
                pass_index: Some(effect.pass_index),
                feature: "we-effect-pass-missing-shader".to_owned(),
                expected_subsystem: "engine::render_graph material pass".to_owned(),
                containment: "suppress-pass-until-material-graph-executor".to_owned(),
            });
        }
        graph.passes.push(node);
    }
    graph
}

pub fn we_effect_pass_node(
    id: u32,
    contract: &WeEffectPassContract,
    final_scene_blend: SceneBlendMode,
) -> RenderPassNode {
    let shader_blend = contract
        .combos
        .get("BLENDMODE")
        .copied()
        .map(ShaderBlendMode::from_we_blendmode);
    let target_name = contract.target.clone();
    let color_blend_passthrough = contract
        .shader
        .as_deref()
        .is_some_and(|shader| shader.contains("effectpassthrough"))
        || contract.effect_file.contains("effectpassthrough");
    RenderPassNode {
        id,
        role: if color_blend_passthrough {
            RenderPassRole::ColorBlendPassthrough
        } else {
            RenderPassRole::EffectMaterial
        },
        object_index: Some(contract.object_index),
        pass_index: contract.pass_index,
        shader: contract.shader.clone(),
        target: target_name
            .as_deref()
            .map(we_render_target_role)
            .unwrap_or(RenderTargetRole::ImageLocalMain),
        target_name,
        target_extent: None,
        bindings: contract
            .binds
            .iter()
            .map(|(slot, binding)| {
                if matches!(binding.as_str(), "previous" | "_previous" | "$previous") {
                    TextureBindingRole::PreviousGraphTarget
                } else if matches!(binding.as_str(), "source" | "g_Texture0") {
                    TextureBindingRole::SourceTexture
                } else if binding.starts_with("_rt_") || binding.starts_with("_alias_") {
                    TextureBindingRole::EffectTarget {
                        name: binding.clone(),
                    }
                } else if binding.starts_with("fbo_") {
                    TextureBindingRole::NamedFboBind {
                        name: binding.clone(),
                    }
                } else {
                    TextureBindingRole::TextureSlot { slot: *slot }
                }
            })
            .collect(),
        state: PassState {
            pipeline_blend: contract
                .material_blending
                .as_deref()
                .map(PipelineBlendMode::from_we_material_blending)
                .unwrap_or(PipelineBlendMode::Normal),
            scene_blend: final_scene_blend,
            shader_blend,
            depth_test: DepthTestMode::from_we_depthtest(contract.depthtest.as_deref()),
            depth_write: contract
                .depthwrite
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case("enabled")),
            cull_mode: CullMode::from_we_cullmode(contract.cullmode.as_deref()),
        },
    }
}

fn we_render_target_role(name: &str) -> RenderTargetRole {
    if name.starts_with("fbo_") {
        RenderTargetRole::NamedFbo
    } else if name.starts_with("_rt_") || name.starts_with("_alias_") {
        RenderTargetRole::FirstClassEffectTarget
    } else {
        RenderTargetRole::NamedFbo
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverse_engineered_material_blend_strings_map_to_pipeline_state() {
        assert_eq!(
            PipelineBlendMode::from_we_material_blending("normal"),
            PipelineBlendMode::Normal
        );
        assert_eq!(
            PipelineBlendMode::from_we_material_blending("translucent"),
            PipelineBlendMode::Translucent
        );
        assert_eq!(
            PipelineBlendMode::from_we_material_blending("additive"),
            PipelineBlendMode::Additive
        );
        assert_eq!(
            PipelineBlendMode::from_we_material_blending("alphatocoverage"),
            PipelineBlendMode::AlphaToCoverage
        );
    }

    #[test]
    fn reverse_engineered_shader_blendmode_table_keeps_special_modes() {
        assert_eq!(
            ShaderBlendMode::from_we_blendmode(28),
            ShaderBlendMode::HslColor
        );
        assert_eq!(
            ShaderBlendMode::from_we_blendmode(30),
            ShaderBlendMode::Tint
        );
        assert_eq!(
            ShaderBlendMode::from_we_blendmode(31),
            ShaderBlendMode::LinearDodge
        );
        assert_eq!(
            ShaderBlendMode::from_we_blendmode(32),
            ShaderBlendMode::Modulate
        );
        assert!(ShaderBlendMode::from_we_blendmode(28).requires_framebuffer_sample());
    }

    #[test]
    fn we_image_graph_keeps_pass_targets_and_derives_barriers() {
        let graph = we_image_graph(&WeImageGraphContract {
            object_index: 7,
            base_shader: Some("genericimage4".to_owned()),
            base_texture_slots: vec![1],
            final_scene_blend: SceneBlendMode::Alpha,
            effect_passes: vec![
                WeEffectPassContract {
                    object_index: 7,
                    effect_file: "effects/waterflow/effect.json".to_owned(),
                    pass_index: 1,
                    shader: Some("effects/waterflow".to_owned()),
                    target: Some("fbo_velocity".to_owned()),
                    binds: [(1, "previous".to_owned())].into_iter().collect(),
                    material_blending: Some("normal".to_owned()),
                    depthtest: Some("disabled".to_owned()),
                    depthwrite: Some("disabled".to_owned()),
                    cullmode: Some("nocull".to_owned()),
                    combos: BTreeMap::new(),
                },
                WeEffectPassContract {
                    object_index: 7,
                    effect_file: "materials/util/effectpassthrough.json".to_owned(),
                    pass_index: 2,
                    shader: Some("util/effectpassthrough".to_owned()),
                    target: None,
                    binds: [(1, "fbo_velocity".to_owned())].into_iter().collect(),
                    material_blending: Some("normal".to_owned()),
                    depthtest: Some("disabled".to_owned()),
                    depthwrite: Some("disabled".to_owned()),
                    cullmode: Some("nocull".to_owned()),
                    combos: [("BLENDMODE".to_owned(), 28)].into_iter().collect(),
                },
            ],
        });

        assert_eq!(graph.passes.len(), 3);
        assert_eq!(graph.passes[1].target, RenderTargetRole::NamedFbo);
        assert_eq!(graph.passes[2].role, RenderPassRole::ColorBlendPassthrough);
        assert_eq!(graph.passes[2].target, RenderTargetRole::SceneColor);
        assert!(
            graph
                .resource_uses()
                .iter()
                .any(|use_| use_.resource_key == "target:named-fbo:fbo_velocity")
        );
        assert!(
            graph
                .derived_barriers()
                .iter()
                .any(|barrier| barrier.resource_key == "target:named-fbo:fbo_velocity")
        );
    }
}
