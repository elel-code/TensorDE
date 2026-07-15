use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::core::SceneBlendMode;

use super::binding::TextureBindingRole;
use super::graph::{RenderGraph, UnsupportedGraphBoundary};
use super::pass::{RenderPassNode, RenderPassRole};
use super::state::{CullMode, DepthTestMode, PassState, PipelineBlendMode, ShaderBlendMode};
use super::target::RenderTargetRole;

mod flat_rounded_mask;
mod foliage_ripple;
mod ripple_flow;
mod waterwaves;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeEffectPassContract {
    pub object_index: usize,
    pub material_index: Option<usize>,
    pub effect_file: String,
    pub pass_index: u32,
    pub command: Option<String>,
    pub shader: Option<String>,
    pub source: Option<String>,
    pub target: Option<String>,
    pub binds: BTreeMap<u32, String>,
    pub pass_constants: Vec<String>,
    pub material_blending: Option<String>,
    pub depthtest: Option<String>,
    pub depthwrite: Option<String>,
    pub cullmode: Option<String>,
    pub combos: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeImageGraphContract {
    pub object_index: usize,
    pub base_material_index: Option<usize>,
    pub base_shader: Option<String>,
    pub base_material_blending: Option<String>,
    pub base_texture_slots: Vec<u32>,
    pub base_pass_constants: Vec<String>,
    pub framebuffer_snapshot: Option<WeFramebufferSnapshotContract>,
    pub final_scene_blend: SceneBlendMode,
    /// Execute image effects in normalized authored-texture coordinates.
    pub effects_in_authored_texture_space: bool,
    /// Apply image-space effects to the authored texture before the puppet mesh deforms it.
    pub puppet_skinning_after_effects: bool,
    /// Converter-authored material containing the complete compatible waterwaves chain.
    pub waterwaves_uv_field_material_index: Option<usize>,
    /// Converter-authored material that evaluates the complete chain in the final mesh fragment.
    pub waterwaves_direct_material: Option<WeWaterWavesDirectMaterial>,
    /// Converter-authored material for a compatible direct foliage/ripple composite.
    pub foliage_ripple_material: Option<WeFoliageRippleMaterial>,
    /// Converter-authored materials for the typed ripple/flow two-stage path.
    pub ripple_flow_material_indices: Option<WeRippleFlowMaterialIndices>,
    /// Converter-authored material evaluated once in the final object draw.
    pub final_effect_material: Option<WeFinalEffectMaterial>,
    pub effect_passes: Vec<WeEffectPassContract>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeRippleFlowMaterialIndices {
    pub ripple_source: usize,
    pub flow_composite: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeWaterWavesDirectMaterial {
    pub material_index: usize,
    pub shader: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeFoliageRippleMaterial {
    pub material_index: usize,
    pub shader: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeFinalEffectMaterial {
    pub material_index: usize,
    pub shader: String,
    pub samples_framebuffer_snapshot: bool,
    pub framebuffer_prepass: Option<WeEffectPassContract>,
}

pub fn we_effect_passes_form_waterwaves_displacement_chain(
    effect_passes: &[WeEffectPassContract],
) -> bool {
    waterwaves::are_compatible_effect_passes(effect_passes)
}

pub fn we_effect_passes_form_foliage_ripple_chain(effect_passes: &[WeEffectPassContract]) -> bool {
    foliage_ripple::are_compatible_effect_passes(effect_passes)
}

pub fn we_effect_passes_form_ripple_flow_chain(effect_passes: &[WeEffectPassContract]) -> bool {
    ripple_flow::are_compatible_effect_passes(effect_passes)
}

pub fn we_image_graph_requires_generated_scene_snapshot(contract: &WeImageGraphContract) -> bool {
    contract.final_scene_blend == SceneBlendMode::HslColor
        && flat_rounded_mask::supports_direct_chain(contract)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeFramebufferSnapshotContract {
    pub target_name: String,
    pub texture_slot: u32,
    pub composite_to_object_mesh: bool,
}

pub fn we_image_graph(contract: &WeImageGraphContract) -> RenderGraph {
    let mut graph = RenderGraph::default();
    let has_effects = !contract.effect_passes.is_empty();
    let composite_to_object_mesh = contract
        .framebuffer_snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot.composite_to_object_mesh);
    let has_offscreen_chain = has_effects || composite_to_object_mesh;
    let authored_texture_effects = has_effects && contract.effects_in_authored_texture_space;
    let puppet_skinning_after_effects =
        authored_texture_effects && contract.puppet_skinning_after_effects;
    let final_pipeline_blend = final_pipeline_blend(contract);
    if flat_rounded_mask::is_compatible(contract) {
        flat_rounded_mask::append_direct_composite(&mut graph, contract);
        return graph;
    }
    if ripple_flow::is_compatible(contract) {
        ripple_flow::append_two_stage_composite(&mut graph, contract);
        return graph;
    }
    if foliage_ripple::is_compatible(contract) {
        foliage_ripple::append_direct_composite(&mut graph, contract);
        return graph;
    }
    if let Some(final_effect) = &contract.final_effect_material {
        if final_effect.samples_framebuffer_snapshot {
            let snapshot = contract
                .framebuffer_snapshot
                .as_ref()
                .expect("framebuffer final effect requires a typed snapshot contract");
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
        }
        if let Some(prepass) = &final_effect.framebuffer_prepass {
            let pass_id = graph.passes.len().min(u32::MAX as usize) as u32;
            let mut node = we_effect_pass_node(pass_id, prepass, SceneBlendMode::Normal);
            let snapshot = contract
                .framebuffer_snapshot
                .as_ref()
                .expect("framebuffer prepass requires a typed snapshot contract");
            for binding in &mut node.bindings {
                if matches!(binding, TextureBindingRole::PreviousGraphTarget { slot: 0 }) {
                    *binding = TextureBindingRole::EffectTarget {
                        slot: snapshot.texture_slot,
                        name: snapshot.target_name.clone(),
                    };
                }
            }
            node.state.scene_blend = SceneBlendMode::Normal;
            graph.passes.push(node);
        }
        let pass_id = graph.passes.len().min(u32::MAX as usize) as u32;
        let bindings = if final_effect.framebuffer_prepass.is_some() {
            vec![TextureBindingRole::PreviousGraphTarget { slot: 0 }]
        } else {
            contract
                .framebuffer_snapshot
                .iter()
                .filter(|_| final_effect.samples_framebuffer_snapshot)
                .map(|snapshot| TextureBindingRole::EffectTarget {
                    slot: snapshot.texture_slot,
                    name: snapshot.target_name.clone(),
                })
                .collect()
        };
        graph.passes.push(RenderPassNode {
            id: pass_id,
            role: RenderPassRole::SceneComposite,
            object_index: Some(contract.object_index),
            material_index: Some(final_effect.material_index),
            pass_index: 0,
            shader: Some(final_effect.shader.clone()),
            target: RenderTargetRole::SceneColor,
            target_name: None,
            target_extent: None,
            target_format: None,
            bindings,
            state: PassState {
                pipeline_blend: final_pipeline_blend,
                scene_blend: contract.final_scene_blend,
                ..PassState::default()
            },
        });
        return graph;
    }
    if let Some(snapshot) = &contract.framebuffer_snapshot {
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
    }
    if waterwaves::is_compatible_displacement_chain(contract) {
        waterwaves::append_displacement_chain(&mut graph, contract);
        return graph;
    }
    let cloudmotion_samples_snapshot_directly =
        framebuffer_cloudmotion_samples_snapshot_directly(contract);
    if !cloudmotion_samples_snapshot_directly {
        let base_pass_id = graph.passes.len().min(u32::MAX as usize) as u32;
        graph.passes.push(RenderPassNode {
            id: base_pass_id,
            role: RenderPassRole::BaseMaterial,
            object_index: Some(contract.object_index),
            material_index: contract.base_material_index,
            pass_index: 0,
            shader: if authored_texture_effects {
                Some(
                    if puppet_skinning_after_effects {
                        "we/puppet-effect-source"
                    } else {
                        "we/image-effect-source"
                    }
                    .to_owned(),
                )
            } else if !has_offscreen_chain
                && contract.final_scene_blend == SceneBlendMode::Multiply
                && contract
                    .base_shader
                    .as_deref()
                    .is_some_and(is_unskinned_generic_image_shader)
            {
                Some("we/genericimage4-multiply-composite".to_owned())
            } else {
                contract.base_shader.clone()
            },
            target: if has_offscreen_chain {
                RenderTargetRole::ImageLocalMain
            } else {
                RenderTargetRole::SceneColor
            },
            target_name: None,
            target_extent: None,
            target_format: None,
            bindings: std::iter::once(TextureBindingRole::SourceTexture)
                .chain(
                    contract
                        .base_texture_slots
                        .iter()
                        .copied()
                        .map(|slot| TextureBindingRole::TextureSlot { slot }),
                )
                .chain(
                    contract
                        .base_pass_constants
                        .iter()
                        .cloned()
                        .map(|name| TextureBindingRole::PassConstant { name }),
                )
                .chain(contract.framebuffer_snapshot.iter().map(|snapshot| {
                    TextureBindingRole::EffectTarget {
                        slot: snapshot.texture_slot,
                        name: snapshot.target_name.clone(),
                    }
                }))
                .collect(),
            state: PassState {
                pipeline_blend: if has_offscreen_chain {
                    base_pipeline_blend(contract)
                } else {
                    final_pipeline_blend
                },
                scene_blend: if has_offscreen_chain {
                    SceneBlendMode::Normal
                } else {
                    contract.final_scene_blend
                },
                ..PassState::default()
            },
        });
    }
    for (index, effect) in contract.effect_passes.iter().enumerate() {
        let pass_id = graph.passes.len().min(u32::MAX as usize) as u32;
        let mut node = we_effect_pass_node(pass_id, effect, contract.final_scene_blend);
        if effect.target.is_none() && node.target == RenderTargetRole::ImageLocalMain {
            node.target = if index % 2 == 0 {
                RenderTargetRole::ImageLocalSub
            } else {
                RenderTargetRole::ImageLocalMain
            };
        }
        if node.target == RenderTargetRole::SceneColor {
            node.state.pipeline_blend = final_pipeline_blend;
        }
        if node.bindings.is_empty() {
            node.bindings
                .push(TextureBindingRole::PreviousGraphTarget { slot: 0 });
        }
        if index == 0 && cloudmotion_samples_snapshot_directly {
            let snapshot = contract
                .framebuffer_snapshot
                .as_ref()
                .expect("direct framebuffer cloudmotion requires snapshot contract");
            for binding in &mut node.bindings {
                if matches!(binding, TextureBindingRole::PreviousGraphTarget { slot: 0 }) {
                    *binding = TextureBindingRole::EffectTarget {
                        slot: snapshot.texture_slot,
                        name: snapshot.target_name.clone(),
                    };
                }
            }
        }
        if node.shader.is_none() && !effect_command_has_no_shader(effect) {
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
    if has_offscreen_chain {
        let pass_id = graph.passes.len().min(u32::MAX as usize) as u32;
        graph.passes.push(RenderPassNode {
            id: pass_id,
            role: RenderPassRole::SceneComposite,
            object_index: Some(contract.object_index),
            material_index: contract.base_material_index,
            pass_index: pass_id,
            shader: Some(
                if puppet_skinning_after_effects {
                    "we/puppet-effect-composite"
                } else if authored_texture_effects {
                    if contract.final_scene_blend == SceneBlendMode::Modulate {
                        "we/image-effect-modulate-composite"
                    } else {
                        "we/image-effect-composite"
                    }
                } else {
                    "we/objectcomposite"
                }
                .to_owned(),
            ),
            target: RenderTargetRole::SceneColor,
            target_name: None,
            target_extent: None,
            target_format: None,
            bindings: vec![TextureBindingRole::PreviousGraphTarget { slot: 0 }],
            state: PassState {
                pipeline_blend: final_pipeline_blend,
                scene_blend: contract.final_scene_blend,
                ..PassState::default()
            },
        });
    }
    graph
}

fn framebuffer_cloudmotion_samples_snapshot_directly(contract: &WeImageGraphContract) -> bool {
    if contract.framebuffer_snapshot.is_none() {
        return false;
    }
    if contract.effects_in_authored_texture_space
        || contract.effect_passes.len() != 1
        || !contract.base_pass_constants.is_empty()
        || contract.base_texture_slots.iter().any(|slot| *slot != 0)
        || !contract
            .base_shader
            .as_deref()
            .is_some_and(|shader| shader.eq_ignore_ascii_case("passthrough"))
    {
        return false;
    }
    let effect = &contract.effect_passes[0];
    effect.command.is_none()
        && effect.source.is_none()
        && effect.target.is_none()
        && effect.shader.as_deref().is_some_and(|shader| {
            shader
                .split("__")
                .next()
                .is_some_and(|shader| shader.eq_ignore_ascii_case("effects/cloudmotion"))
        })
        && effect
            .binds
            .get(&0)
            .is_some_and(|source| matches!(source.as_str(), "previous" | "_previous" | "$previous"))
}

fn is_unskinned_generic_image_shader(shader: &str) -> bool {
    !shader.to_ascii_lowercase().contains("puppetskinning")
        && matches!(
            shader
                .split("__")
                .next()
                .unwrap_or_default()
                .rsplit('/')
                .next()
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str(),
            "genericimage2" | "genericimage4"
        )
}

fn base_pipeline_blend(contract: &WeImageGraphContract) -> PipelineBlendMode {
    contract
        .base_material_blending
        .as_deref()
        .map(PipelineBlendMode::from_we_material_blending)
        .unwrap_or(PipelineBlendMode::Normal)
}

fn final_pipeline_blend(contract: &WeImageGraphContract) -> PipelineBlendMode {
    match contract.final_scene_blend {
        SceneBlendMode::Additive => PipelineBlendMode::Additive,
        SceneBlendMode::AlphaToCoverage => PipelineBlendMode::AlphaToCoverage,
        _ => base_pipeline_blend(contract),
    }
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
    let command_role = effect_command_role(contract.command.as_deref());
    RenderPassNode {
        id,
        role: command_role.unwrap_or(if color_blend_passthrough {
            RenderPassRole::ColorBlendPassthrough
        } else {
            RenderPassRole::EffectMaterial
        }),
        object_index: Some(contract.object_index),
        material_index: contract.material_index,
        pass_index: contract.pass_index,
        shader: contract.shader.clone(),
        target: target_name
            .as_deref()
            .map(we_render_target_role)
            .unwrap_or(RenderTargetRole::ImageLocalMain),
        target_name,
        target_extent: None,
        target_format: None,
        bindings: contract
            .binds
            .iter()
            .map(|(slot, binding)| we_binding_role(*slot, binding))
            .chain(
                contract
                    .source
                    .iter()
                    .map(|source| we_binding_role(0, source)),
            )
            .chain(
                contract
                    .pass_constants
                    .iter()
                    .cloned()
                    .map(|name| TextureBindingRole::PassConstant { name }),
            )
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

fn effect_command_role(command: Option<&str>) -> Option<RenderPassRole> {
    let command = command?;
    if command.eq_ignore_ascii_case("copy") {
        Some(RenderPassRole::CopyTarget)
    } else if command.eq_ignore_ascii_case("swap") {
        Some(RenderPassRole::SwapTargetReferences)
    } else {
        None
    }
}

fn effect_command_has_no_shader(contract: &WeEffectPassContract) -> bool {
    effect_command_role(contract.command.as_deref()).is_some()
}

fn we_binding_role(slot: u32, binding: &str) -> TextureBindingRole {
    if matches!(binding, "previous" | "_previous" | "$previous") {
        TextureBindingRole::PreviousGraphTarget { slot }
    } else if matches!(binding, "source" | "g_Texture0") {
        TextureBindingRole::SourceTexture
    } else if binding.starts_with("_rt_")
        || binding.starts_with("_alias_")
        || binding.starts_with("_tmp_")
    {
        TextureBindingRole::EffectTarget {
            slot,
            name: binding.to_owned(),
        }
    } else if binding.starts_with("fbo_") {
        TextureBindingRole::NamedFboBind {
            slot,
            name: binding.to_owned(),
        }
    } else {
        TextureBindingRole::TextureSlot { slot }
    }
}

fn we_render_target_role(name: &str) -> RenderTargetRole {
    if name.starts_with("fbo_") {
        RenderTargetRole::NamedFbo
    } else if name.starts_with("_tmp_") {
        RenderTargetRole::Temporary
    } else if name.starts_with("_rt_") || name.starts_with("_alias_") {
        RenderTargetRole::FirstClassEffectTarget
    } else {
        RenderTargetRole::NamedFbo
    }
}

#[cfg(test)]
#[path = "we_image/tests.rs"]
mod tests;
