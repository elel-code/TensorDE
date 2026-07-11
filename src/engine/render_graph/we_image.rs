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
    pub effect_passes: Vec<WeEffectPassContract>,
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
                    "we/image-effect-composite"
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
    } else if binding.starts_with("_rt_") || binding.starts_with("_alias_") {
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
            base_material_index: Some(3),
            base_shader: Some("genericimage4".to_owned()),
            base_material_blending: Some("translucent".to_owned()),
            base_texture_slots: vec![1],
            base_pass_constants: vec!["tint".to_owned()],
            framebuffer_snapshot: None,
            final_scene_blend: SceneBlendMode::Alpha,
            effects_in_authored_texture_space: false,
            puppet_skinning_after_effects: false,
            effect_passes: vec![
                WeEffectPassContract {
                    object_index: 7,
                    material_index: Some(4),
                    effect_file: "effects/waterflow/effect.json".to_owned(),
                    pass_index: 1,
                    command: None,
                    shader: Some("effects/waterflow".to_owned()),
                    source: None,
                    target: Some("fbo_velocity".to_owned()),
                    binds: [(1, "previous".to_owned())].into_iter().collect(),
                    pass_constants: vec!["speed".to_owned()],
                    material_blending: Some("normal".to_owned()),
                    depthtest: Some("disabled".to_owned()),
                    depthwrite: Some("disabled".to_owned()),
                    cullmode: Some("nocull".to_owned()),
                    combos: BTreeMap::new(),
                },
                WeEffectPassContract {
                    object_index: 7,
                    material_index: Some(5),
                    effect_file: "materials/util/effectpassthrough.json".to_owned(),
                    pass_index: 2,
                    command: None,
                    shader: Some("util/effectpassthrough".to_owned()),
                    source: None,
                    target: None,
                    binds: [(1, "fbo_velocity".to_owned())].into_iter().collect(),
                    pass_constants: Vec::new(),
                    material_blending: Some("normal".to_owned()),
                    depthtest: Some("disabled".to_owned()),
                    depthwrite: Some("disabled".to_owned()),
                    cullmode: Some("nocull".to_owned()),
                    combos: [("BLENDMODE".to_owned(), 28)].into_iter().collect(),
                },
            ],
        });

        assert_eq!(graph.passes.len(), 4);
        assert_eq!(
            graph.passes[0].state.pipeline_blend,
            PipelineBlendMode::Translucent
        );
        assert_eq!(graph.passes[0].material_index, Some(3));
        assert!(
            graph.passes[0]
                .bindings
                .contains(&TextureBindingRole::PassConstant {
                    name: "tint".to_owned()
                })
        );
        assert_eq!(graph.passes[1].material_index, Some(4));
        assert!(
            graph.passes[1]
                .bindings
                .contains(&TextureBindingRole::PreviousGraphTarget { slot: 1 })
        );
        assert!(
            graph.passes[1]
                .bindings
                .contains(&TextureBindingRole::PassConstant {
                    name: "speed".to_owned()
                })
        );
        assert_eq!(graph.passes[1].target, RenderTargetRole::NamedFbo);
        assert_eq!(graph.passes[2].role, RenderPassRole::ColorBlendPassthrough);
        assert_eq!(graph.passes[2].target, RenderTargetRole::ImageLocalMain);
        assert_eq!(
            graph.passes[2].state.pipeline_blend,
            PipelineBlendMode::Normal
        );
        assert!(
            graph.passes[2]
                .bindings
                .contains(&TextureBindingRole::NamedFboBind {
                    slot: 1,
                    name: "fbo_velocity".to_owned(),
                })
        );
        assert_eq!(graph.passes[3].role, RenderPassRole::SceneComposite);
        assert_eq!(graph.passes[3].target, RenderTargetRole::SceneColor);
        assert_eq!(
            graph.passes[3].shader.as_deref(),
            Some("we/objectcomposite")
        );
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

    #[test]
    fn effect_target_base_keeps_authored_translucent_submesh_assembly() {
        let graph = we_image_graph(&WeImageGraphContract {
            object_index: 7,
            base_material_index: Some(3),
            base_shader: Some("genericimage4".to_owned()),
            base_material_blending: Some("translucent".to_owned()),
            base_texture_slots: vec![0],
            base_pass_constants: Vec::new(),
            framebuffer_snapshot: None,
            final_scene_blend: SceneBlendMode::Alpha,
            effects_in_authored_texture_space: false,
            puppet_skinning_after_effects: false,
            effect_passes: vec![WeEffectPassContract {
                object_index: 7,
                material_index: Some(4),
                effect_file: "effects/waterwaves/effect.json".to_owned(),
                pass_index: 0,
                command: None,
                shader: Some("effects/waterwaves".to_owned()),
                source: None,
                target: None,
                binds: BTreeMap::new(),
                pass_constants: Vec::new(),
                material_blending: Some("normal".to_owned()),
                depthtest: None,
                depthwrite: None,
                cullmode: None,
                combos: BTreeMap::new(),
            }],
        });

        assert_eq!(
            graph.passes[0].state.pipeline_blend,
            PipelineBlendMode::Translucent
        );
        assert_eq!(graph.passes[0].target, RenderTargetRole::ImageLocalMain);
        assert_eq!(graph.passes[1].target, RenderTargetRole::ImageLocalSub);
        assert_eq!(graph.passes[2].role, RenderPassRole::SceneComposite);
        assert_eq!(graph.passes[2].target, RenderTargetRole::SceneColor);
    }

    #[test]
    fn puppet_image_effects_run_before_skinning_composite() {
        let graph = we_image_graph(&WeImageGraphContract {
            object_index: 7,
            base_material_index: Some(3),
            base_shader: Some("we/genericimage4__PUPPETSKINNING_1".to_owned()),
            base_material_blending: Some("translucent".to_owned()),
            base_texture_slots: vec![0],
            base_pass_constants: Vec::new(),
            framebuffer_snapshot: None,
            final_scene_blend: SceneBlendMode::Alpha,
            effects_in_authored_texture_space: true,
            puppet_skinning_after_effects: true,
            effect_passes: vec![WeEffectPassContract {
                object_index: 7,
                material_index: Some(4),
                effect_file: "effects/waterwaves/effect.json".to_owned(),
                pass_index: 0,
                command: None,
                shader: Some("effects/waterwaves__SLOTS_3".to_owned()),
                source: None,
                target: None,
                binds: [(0, "previous".to_owned())].into_iter().collect(),
                pass_constants: Vec::new(),
                material_blending: Some("normal".to_owned()),
                depthtest: None,
                depthwrite: None,
                cullmode: None,
                combos: BTreeMap::new(),
            }],
        });

        assert_eq!(graph.passes.len(), 3);
        assert_eq!(
            graph.passes[0].shader.as_deref(),
            Some("we/puppet-effect-source")
        );
        assert_eq!(
            graph.passes[2].shader.as_deref(),
            Some("we/puppet-effect-composite")
        );
    }

    #[test]
    fn we_image_graph_keeps_effect_copy_and_swap_command_passes() {
        let graph = we_image_graph(&WeImageGraphContract {
            object_index: 9,
            base_material_index: None,
            base_shader: Some("genericimage4".to_owned()),
            base_material_blending: None,
            base_texture_slots: Vec::new(),
            base_pass_constants: Vec::new(),
            framebuffer_snapshot: None,
            final_scene_blend: SceneBlendMode::Alpha,
            effects_in_authored_texture_space: false,
            puppet_skinning_after_effects: false,
            effect_passes: vec![
                WeEffectPassContract {
                    object_index: 9,
                    material_index: None,
                    effect_file: "effects/fluid/effect.json".to_owned(),
                    pass_index: 1,
                    command: Some("copy".to_owned()),
                    shader: None,
                    source: Some("fbo_src".to_owned()),
                    target: Some("fbo_dst".to_owned()),
                    binds: BTreeMap::new(),
                    pass_constants: Vec::new(),
                    material_blending: None,
                    depthtest: None,
                    depthwrite: None,
                    cullmode: None,
                    combos: BTreeMap::new(),
                },
                WeEffectPassContract {
                    object_index: 9,
                    material_index: None,
                    effect_file: "effects/fluid/effect.json".to_owned(),
                    pass_index: 2,
                    command: Some("swap".to_owned()),
                    shader: None,
                    source: Some("fbo_a".to_owned()),
                    target: Some("fbo_b".to_owned()),
                    binds: BTreeMap::new(),
                    pass_constants: Vec::new(),
                    material_blending: None,
                    depthtest: None,
                    depthwrite: None,
                    cullmode: None,
                    combos: BTreeMap::new(),
                },
            ],
        });

        assert_eq!(graph.passes[1].role, RenderPassRole::CopyTarget);
        assert_eq!(graph.passes[2].role, RenderPassRole::SwapTargetReferences);
        assert!(graph.unsupported.is_empty());
        assert!(
            graph
                .resource_uses()
                .iter()
                .any(|use_| use_.resource_key == "target:named-fbo:fbo_src")
        );
    }

    #[test]
    fn framebuffer_utility_graph_snapshots_scene_color_before_sampling_it() {
        let graph = we_image_graph(&WeImageGraphContract {
            object_index: 11,
            base_material_index: Some(4),
            base_shader: Some("passthrough".to_owned()),
            base_material_blending: Some("translucent".to_owned()),
            base_texture_slots: vec![0],
            base_pass_constants: Vec::new(),
            framebuffer_snapshot: Some(WeFramebufferSnapshotContract {
                target_name: "_rt_FullFrameBuffer".to_owned(),
                texture_slot: 0,
                composite_to_object_mesh: false,
            }),
            final_scene_blend: SceneBlendMode::Alpha,
            effects_in_authored_texture_space: false,
            puppet_skinning_after_effects: false,
            effect_passes: Vec::new(),
        });

        assert_eq!(graph.passes.len(), 2);
        assert_eq!(graph.passes[0].role, RenderPassRole::CopyTarget);
        assert_eq!(
            graph.passes[0].target,
            RenderTargetRole::FirstClassEffectTarget
        );
        assert_eq!(graph.passes[1].role, RenderPassRole::BaseMaterial);
        assert_eq!(graph.passes[1].target, RenderTargetRole::SceneColor);
        assert!(
            graph.passes[1]
                .bindings
                .contains(&TextureBindingRole::EffectTarget {
                    slot: 0,
                    name: "_rt_FullFrameBuffer".to_owned(),
                })
        );
    }

    #[test]
    fn composelayer_effect_chain_composites_back_through_the_object_mesh() {
        let graph = we_image_graph(&WeImageGraphContract {
            object_index: 12,
            base_material_index: Some(5),
            base_shader: Some("composelayer".to_owned()),
            base_material_blending: Some("translucent".to_owned()),
            base_texture_slots: vec![0],
            base_pass_constants: Vec::new(),
            framebuffer_snapshot: Some(WeFramebufferSnapshotContract {
                target_name: "_rt_FullFrameBuffer".to_owned(),
                texture_slot: 0,
                composite_to_object_mesh: true,
            }),
            final_scene_blend: SceneBlendMode::Alpha,
            effects_in_authored_texture_space: false,
            puppet_skinning_after_effects: false,
            effect_passes: vec![WeEffectPassContract {
                object_index: 12,
                material_index: Some(6),
                effect_file: "effects/opacity/effect.json".to_owned(),
                pass_index: 0,
                command: None,
                shader: Some("effects/opacity__SLOTS_1".to_owned()),
                source: None,
                target: None,
                binds: [(0, "previous".to_owned())].into_iter().collect(),
                pass_constants: vec!["alpha".to_owned()],
                material_blending: Some("normal".to_owned()),
                depthtest: None,
                depthwrite: None,
                cullmode: None,
                combos: BTreeMap::new(),
            }],
        });

        assert_eq!(graph.passes.len(), 4);
        assert_eq!(graph.passes[2].target, RenderTargetRole::ImageLocalSub);
        assert_eq!(graph.passes[3].role, RenderPassRole::SceneComposite);
        assert_eq!(
            graph.passes[3].shader.as_deref(),
            Some("we/objectcomposite")
        );
        assert_eq!(graph.passes[3].target, RenderTargetRole::SceneColor);
    }
}
