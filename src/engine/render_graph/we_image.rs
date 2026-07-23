use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::core::SceneBlendMode;

use super::binding::TextureBindingRole;
use super::graph::{RenderGraph, RenderGraphActivationPolicy, UnsupportedGraphBoundary};
use super::pass::{
    RenderPassDrawPrimitive, RenderPassEffectVisibility, RenderPassNode, RenderPassRole,
};
use super::state::{
    ColorWriteMask, CullMode, DepthTestMode, PassState, PipelineBlendMode, ShaderBlendMode,
};
use super::target::RenderTargetRole;

mod flat_rounded_mask;
mod foliage_ripple;
mod ripple_flow;
mod waterwaves;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeEffectPassContract {
    pub object_index: usize,
    pub effect_binding_start: u32,
    pub effect_binding_count: u32,
    /// Whether this render stage still needs semantic-ECS effect visibility at runtime.
    pub runtime_visibility: bool,
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
    /// The authored object color is an unbound literal black value.
    pub static_black_output: bool,
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
    pub group_visual_composite: bool,
}

fn contiguous_effect_range(effects: &[WeEffectPassContract]) -> Option<(u32, u32)> {
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
        return Some((u32::MAX, 0));
    }
    if effects.iter().any(|effect| !effect.runtime_visibility) {
        return None;
    }
    let start = first.effect_binding_start;
    let mut next = start;
    for effect in effects {
        if effect.effect_binding_start != next || effect.effect_binding_count == 0 {
            return None;
        }
        next = next.checked_add(effect.effect_binding_count)?;
    }
    Some((start, next - start))
}

fn contiguous_material_stage_visibility(
    effects: &[WeEffectPassContract],
) -> Option<RenderPassEffectVisibility> {
    let (start, count) = contiguous_effect_range(effects)?;
    Some(if count == 0 {
        RenderPassEffectVisibility::NONE
    } else {
        RenderPassEffectVisibility::material_stages(start, count)
    })
}

fn material_stage_range_visibility(
    effects: &[WeEffectPassContract],
    stage_index: usize,
    stage_count: usize,
) -> Option<RenderPassEffectVisibility> {
    let end = stage_index.checked_add(stage_count)?;
    if stage_count == 0 || end > effects.len() {
        return None;
    }
    contiguous_material_stage_visibility(&effects[stage_index..end])
}

fn single_effect_visibility(
    effect: &WeEffectPassContract,
    dynamic: impl FnOnce(u32, u32) -> RenderPassEffectVisibility,
) -> RenderPassEffectVisibility {
    if effect.runtime_visibility {
        dynamic(effect.effect_binding_start, effect.effect_binding_count)
    } else {
        RenderPassEffectVisibility::NONE
    }
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
    pub draw_primitive: RenderPassDrawPrimitive,
    pub effect_stage_index: usize,
    pub effect_stage_count: usize,
    pub prepass: Option<WeFinalEffectPrepass>,
    pub intermediate: Option<WeFinalEffectIntermediate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeFinalEffectPrepass {
    pub material_index: usize,
    pub shader: String,
    pub effect_stage_index: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeFinalEffectIntermediate {
    pub material_index: usize,
    pub shader: String,
    pub effect_stage_index: usize,
    pub effect_stage_count: usize,
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
    pub usage: WeFramebufferSnapshotUsage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WeFramebufferSnapshotUsage {
    ObjectSource,
    EffectOnlyLayer,
    EffectShaderInput,
}

pub fn we_image_graph(contract: &WeImageGraphContract) -> RenderGraph {
    let mut graph = RenderGraph::default();
    let has_effects = !contract.effect_passes.is_empty();
    let effect_only_layer = contract
        .framebuffer_snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot.usage == WeFramebufferSnapshotUsage::EffectOnlyLayer);
    if effect_only_layer && !has_effects {
        return graph;
    }
    if effect_only_layer
        && contract
            .effect_passes
            .iter()
            .all(|effect| effect.runtime_visibility)
    {
        graph.activation_policy = RenderGraphActivationPolicy::AnyEffectVisible;
    }
    let composite_to_object_mesh = contract
        .framebuffer_snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot.composite_to_object_mesh);
    let has_offscreen_chain = has_effects || composite_to_object_mesh;
    let final_effect_composites_to_scene = effect_only_layer
        && contract
            .effect_passes
            .last()
            .is_some_and(effect_only_final_material_composites_to_scene);
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
        if let Some(prepass) = &final_effect.prepass {
            let snapshot = contract
                .framebuffer_snapshot
                .as_ref()
                .expect("typed final-effect prepass requires a framebuffer snapshot");
            let effect = contract
                .effect_passes
                .get(prepass.effect_stage_index)
                .expect("typed final-effect prepass references a missing effect stage");
            graph.passes.push(RenderPassNode {
                id: 0,
                role: RenderPassRole::CopyTarget,
                draw_primitive: RenderPassDrawPrimitive::None,
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
                effect_visibility: RenderPassEffectVisibility::NONE,
                state: PassState::default(),
            });
            graph.passes.push(RenderPassNode {
                id: 1,
                role: RenderPassRole::EffectMaterial,
                draw_primitive: RenderPassDrawPrimitive::FullscreenTriangle,
                object_index: Some(contract.object_index),
                material_index: Some(prepass.material_index),
                pass_index: effect.pass_index,
                shader: Some(prepass.shader.clone()),
                target: RenderTargetRole::ImageLocalMain,
                target_name: None,
                target_extent: None,
                target_format: Some("rgba8".to_owned()),
                bindings: vec![TextureBindingRole::EffectTarget {
                    slot: 0,
                    name: snapshot.target_name.clone(),
                }],
                effect_visibility: single_effect_visibility(
                    effect,
                    RenderPassEffectVisibility::passthrough,
                ),
                state: PassState {
                    pipeline_blend: PipelineBlendMode::Normal,
                    scene_blend: SceneBlendMode::Normal,
                    ..PassState::default()
                },
            });
        }
        if let Some(intermediate) = &final_effect.intermediate {
            assert!(
                final_effect.prepass.is_some(),
                "typed final-effect intermediate requires an earlier prepass"
            );
            let effect = contract
                .effect_passes
                .get(intermediate.effect_stage_index)
                .expect("typed final-effect intermediate references a missing effect stage");
            let pass_id = graph.passes.len().min(u32::MAX as usize) as u32;
            graph.passes.push(RenderPassNode {
                id: pass_id,
                role: RenderPassRole::EffectMaterial,
                draw_primitive: RenderPassDrawPrimitive::FullscreenTriangle,
                object_index: Some(contract.object_index),
                material_index: Some(intermediate.material_index),
                pass_index: effect.pass_index,
                shader: Some(intermediate.shader.clone()),
                target: RenderTargetRole::ImageLocalSub,
                target_name: None,
                target_extent: None,
                target_format: Some("rgba8".to_owned()),
                bindings: vec![TextureBindingRole::PreviousGraphTarget { slot: 0 }],
                effect_visibility: material_stage_range_visibility(
                    &contract.effect_passes,
                    intermediate.effect_stage_index,
                    intermediate.effect_stage_count,
                )
                .expect("typed final-effect intermediate requires contiguous effect bindings"),
                state: PassState {
                    pipeline_blend: PipelineBlendMode::Normal,
                    scene_blend: SceneBlendMode::Normal,
                    ..PassState::default()
                },
            });
        }
        let effect_visibility = material_stage_range_visibility(
            &contract.effect_passes,
            final_effect.effect_stage_index,
            final_effect.effect_stage_count,
        )
        .expect("typed final effect requires contiguous effect bindings");
        let pass_id = graph.passes.len().min(u32::MAX as usize) as u32;
        graph.passes.push(RenderPassNode {
            id: pass_id,
            role: RenderPassRole::SceneComposite,
            draw_primitive: final_effect.draw_primitive,
            object_index: Some(contract.object_index),
            material_index: Some(final_effect.material_index),
            pass_index: 0,
            shader: Some(final_effect.shader.clone()),
            target: RenderTargetRole::SceneColor,
            target_name: None,
            target_extent: None,
            target_format: None,
            bindings: final_effect
                .prepass
                .as_ref()
                .map(|_| vec![TextureBindingRole::PreviousGraphTarget { slot: 0 }])
                .unwrap_or_default(),
            effect_visibility,
            state: PassState {
                pipeline_blend: if final_effect.prepass.is_some() {
                    PipelineBlendMode::Translucent
                } else {
                    final_pipeline_blend
                },
                scene_blend: contract.final_scene_blend,
                color_write_mask: if final_effect.prepass.is_some() {
                    ColorWriteMask::Rgb
                } else {
                    ColorWriteMask::Rgba
                },
                ..PassState::default()
            },
        });
        return graph;
    }
    if let Some(snapshot) = &contract.framebuffer_snapshot {
        graph.passes.push(RenderPassNode {
            id: 0,
            role: RenderPassRole::CopyTarget,
            draw_primitive: RenderPassDrawPrimitive::None,
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
            effect_visibility: RenderPassEffectVisibility::NONE,
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
            draw_primitive: RenderPassDrawPrimitive::ObjectMesh,
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
                .chain(
                    contract
                        .framebuffer_snapshot
                        .iter()
                        .filter(|snapshot| {
                            snapshot.usage != WeFramebufferSnapshotUsage::EffectShaderInput
                        })
                        .map(|snapshot| TextureBindingRole::EffectTarget {
                            slot: snapshot.texture_slot,
                            name: snapshot.target_name.clone(),
                        }),
                )
                .collect(),
            effect_visibility: RenderPassEffectVisibility::NONE,
            state: PassState {
                pipeline_blend: if has_offscreen_chain {
                    if effect_only_layer {
                        PipelineBlendMode::Normal
                    } else {
                        base_pipeline_blend(contract)
                    }
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
        let is_final_effect_scene_composite =
            final_effect_composites_to_scene && index + 1 == contract.effect_passes.len();
        if is_final_effect_scene_composite {
            node.role = RenderPassRole::SceneComposite;
            node.target = RenderTargetRole::SceneColor;
            node.target_name = None;
            node.state.pipeline_blend = PipelineBlendMode::Translucent;
            node.state.color_write_mask = ColorWriteMask::Rgb;
        }
        if node.target == RenderTargetRole::SceneColor && !is_final_effect_scene_composite {
            node.state.pipeline_blend = final_pipeline_blend;
        }
        if node.effect_visibility.policy
            == super::pass::RenderPassEffectVisibilityPolicy::Passthrough
            && !node
                .bindings
                .iter()
                .any(|binding| texture_binding_uses_slot(binding, 0))
        {
            node.bindings
                .push(TextureBindingRole::PreviousGraphTarget { slot: 0 });
        } else if node.bindings.is_empty() {
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
    if has_offscreen_chain && !final_effect_composites_to_scene {
        let pass_id = graph.passes.len().min(u32::MAX as usize) as u32;
        graph.passes.push(RenderPassNode {
            id: pass_id,
            role: RenderPassRole::SceneComposite,
            draw_primitive: if authored_texture_effects {
                RenderPassDrawPrimitive::ObjectMesh
            } else {
                RenderPassDrawPrimitive::FullscreenTriangle
            },
            object_index: Some(contract.object_index),
            material_index: contract.base_material_index,
            pass_index: pass_id,
            shader: Some(
                if puppet_skinning_after_effects {
                    "we/puppet-effect-composite"
                } else if authored_texture_effects {
                    if contract.final_scene_blend == SceneBlendMode::Modulate {
                        "we/image-effect-modulate-composite"
                    } else if contract.final_scene_blend == SceneBlendMode::Multiply
                        && contract.static_black_output
                    {
                        "we/image-effect-composite__STATIC_BLACK_1"
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
            effect_visibility: RenderPassEffectVisibility::NONE,
            state: PassState {
                pipeline_blend: final_pipeline_blend,
                scene_blend: contract.final_scene_blend,
                ..PassState::default()
            },
        });
    }
    graph
}

/// WE binds the normal scene target for the last material in an effect-only layer unless the
/// authored pass names another target. The D3D11 instruction stream is the runtime authority for
/// this boundary; copy/swap and named-target commands remain explicit graph operations.
fn effect_only_final_material_composites_to_scene(effect: &WeEffectPassContract) -> bool {
    effect.command.is_none()
        && effect.target.is_none()
        && effect.material_index.is_some()
        && effect.shader.is_some()
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
    let role = command_role.unwrap_or(if color_blend_passthrough {
        RenderPassRole::ColorBlendPassthrough
    } else {
        RenderPassRole::EffectMaterial
    });
    let draw_primitive = match role {
        RenderPassRole::EffectMaterial | RenderPassRole::ColorBlendPassthrough => {
            RenderPassDrawPrimitive::FullscreenTriangle
        }
        RenderPassRole::CopyTarget | RenderPassRole::SwapTargetReferences => {
            RenderPassDrawPrimitive::None
        }
        _ => unreachable!("effect pass lowering produced a non-effect render role"),
    };
    let effect_visibility = if !contract.runtime_visibility {
        RenderPassEffectVisibility::NONE
    } else if contract.effect_binding_count > 1
        && contract
            .shader
            .as_deref()
            .is_some_and(|shader| shader.starts_with("we/effect-waterwaves-direct"))
    {
        RenderPassEffectVisibility::waterwaves_stages(
            contract.effect_binding_start,
            contract.effect_binding_count,
        )
    } else if matches!(
        role,
        RenderPassRole::EffectMaterial | RenderPassRole::ColorBlendPassthrough
    ) {
        RenderPassEffectVisibility::passthrough(
            contract.effect_binding_start,
            contract.effect_binding_count,
        )
    } else {
        RenderPassEffectVisibility::NONE
    };
    RenderPassNode {
        id,
        role,
        draw_primitive,
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
        effect_visibility,
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
            ..PassState::default()
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

fn texture_binding_uses_slot(binding: &TextureBindingRole, expected: u32) -> bool {
    match binding {
        TextureBindingRole::SourceTexture => expected == 0,
        TextureBindingRole::TextureSlot { slot }
        | TextureBindingRole::AlphaTextureSlot { slot }
        | TextureBindingRole::PreviousGraphTarget { slot }
        | TextureBindingRole::GraphTarget { slot, .. }
        | TextureBindingRole::NamedFboBind { slot, .. }
        | TextureBindingRole::EffectTarget { slot, .. } => *slot == expected,
        TextureBindingRole::VideoFrame { .. }
        | TextureBindingRole::AudioUniform
        | TextureBindingRole::SystemUniform
        | TextureBindingRole::PassConstant { .. } => false,
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
