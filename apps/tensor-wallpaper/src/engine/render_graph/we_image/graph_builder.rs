use super::*;

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
    if scene_color_blend::is_compatible(contract) {
        scene_color_blend::append_authored_source_and_composite(&mut graph, contract);
        return graph;
    }
    if flat_rounded_mask::is_compatible(contract) {
        flat_rounded_mask::append_direct_composite(&mut graph, contract);
        return graph;
    }
    if ripple_flow::is_compatible(contract) {
        ripple_flow::append_authored_terminal_flow_chain(&mut graph, contract);
        return graph;
    }
    if foliage_ripple::is_compatible(contract) {
        foliage_ripple::append_direct_composite(&mut graph, contract);
        return graph;
    }
    if final_effect::append(&mut graph, contract) {
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
            role: if authored_texture_effects {
                RenderPassRole::ObjectLocalSource
            } else {
                RenderPassRole::BaseMaterial
            },
            draw_primitive: if authored_texture_effects {
                RenderPassDrawPrimitive::ObjectUvSupportQuad
            } else {
                RenderPassDrawPrimitive::ObjectMesh
            },
            object_index: Some(contract.object_index),
            material_index: contract.base_material_index,
            pass_index: 0,
            shader: if authored_texture_effects {
                Some("we/image-effect-source".to_owned())
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
                    PipelineBlendMode::Normal
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
            if composite_to_object_mesh {
                node.draw_primitive = RenderPassDrawPrimitive::ObjectMesh;
            }
            node.target = RenderTargetRole::SceneColor;
            node.target_name = None;
            node.state.pipeline_blend = PipelineBlendMode::Translucent;
            node.state.color_write_mask = ColorWriteMask::Rgb;
        }
        if node.target == RenderTargetRole::SceneColor && !is_final_effect_scene_composite {
            node.state.pipeline_blend = final_pipeline_blend;
        }
        let missing_passthrough_source = node.effect_visibility.policy
            == super::super::pass::RenderPassEffectVisibilityPolicy::Passthrough
            && !node
                .bindings
                .iter()
                .any(|binding| texture_binding_uses_slot(binding, 0));
        if missing_passthrough_source || node.bindings.is_empty() {
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
                pipeline_blend: if puppet_skinning_after_effects {
                    PipelineBlendMode::Translucent
                } else {
                    final_pipeline_blend
                },
                scene_blend: contract.final_scene_blend,
                color_write_mask: if puppet_skinning_after_effects {
                    ColorWriteMask::Rgb
                } else {
                    ColorWriteMask::Rgba
                },
                ..PassState::default()
            },
        });
    }
    let static_terminal_promoted = promote_static_terminal_effect_to_scene(&mut graph, contract);
    let internal_runtime_effect_branched =
        append_single_internal_runtime_effect_branches(&mut graph, contract);
    let terminal_routed = static_terminal_promoted
        || internal_runtime_effect_branched
        || append_runtime_terminal_effect_bypass(&mut graph, contract);
    if !terminal_routed
        && let Some((binding_start, binding_count)) = runtime_direct_bypass_effect_range(contract)
    {
        let active_chain = RenderPassEffectVisibility::any_visible(binding_start, binding_count);
        for pass in &mut graph.passes {
            pass.effect_visibility = active_chain;
        }
        let pass_id = graph.passes.len().min(u32::MAX as usize) as u32;
        graph.passes.push(RenderPassNode {
            id: pass_id,
            role: RenderPassRole::BaseMaterial,
            draw_primitive: RenderPassDrawPrimitive::ObjectMesh,
            object_index: Some(contract.object_index),
            material_index: contract.base_material_index,
            pass_index: 0,
            shader: Some(direct_base_shader(contract)),
            target: RenderTargetRole::SceneColor,
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
                .collect(),
            effect_visibility: RenderPassEffectVisibility::none_visible(
                binding_start,
                binding_count,
            ),
            state: PassState {
                pipeline_blend: final_pipeline_blend,
                scene_blend: contract.final_scene_blend,
                ..PassState::default()
            },
        });
    }
    graph
}

fn promote_static_terminal_effect_to_scene(
    graph: &mut RenderGraph,
    contract: &WeImageGraphContract,
) -> bool {
    if contract.puppet_skinning_after_effects
        || contract.final_scene_blend != SceneBlendMode::Alpha
        || contract.static_black_output
    {
        return false;
    }
    let Some(effect) = contract.effect_passes.last() else {
        return false;
    };
    if contract
        .framebuffer_snapshot
        .as_ref()
        .is_some_and(|snapshot| snapshot.usage == WeFramebufferSnapshotUsage::EffectOnlyLayer)
        && effect_only_final_material_composites_to_scene(effect)
    {
        // The effect-only route already promoted this authored terminal to SceneColor.
        // It is not the synthetic object-composite pass that this optimization can remove.
        return false;
    }
    if effect.runtime_visibility
        || effect.command.is_some()
        || effect.source.is_some()
        || effect.target.is_some()
        || effect.material_index.is_none()
        || effect.shader.is_none()
        || !effect
            .binds
            .get(&0)
            .is_some_and(|source| matches!(source.as_str(), "previous" | "_previous" | "$previous"))
    {
        return false;
    }
    let Some(terminal_index) = graph.passes.len().checked_sub(1) else {
        return false;
    };
    let Some(effect_index) = terminal_index.checked_sub(1) else {
        return false;
    };
    let terminal = &graph.passes[terminal_index];
    let effect_pass = &graph.passes[effect_index];
    if terminal.role != RenderPassRole::SceneComposite
        || terminal.target != RenderTargetRole::SceneColor
        || effect_pass.role != RenderPassRole::EffectMaterial
        || effect_pass.effect_visibility != RenderPassEffectVisibility::NONE
        || !matches!(
            effect_pass.target,
            RenderTargetRole::ImageLocalMain | RenderTargetRole::ImageLocalSub
        )
        || effect_pass.target_name.is_some()
    {
        return false;
    }
    let terminal_state = terminal.state.clone();
    let final_effect = &mut graph.passes[effect_index];
    final_effect.role = RenderPassRole::SceneComposite;
    final_effect.target = RenderTargetRole::SceneColor;
    if contract.effects_in_authored_texture_space {
        final_effect.draw_primitive = RenderPassDrawPrimitive::ObjectMesh;
    }
    final_effect.state.pipeline_blend = terminal_state.pipeline_blend;
    final_effect.state.scene_blend = terminal_state.scene_blend;
    final_effect.state.color_write_mask = terminal_state.color_write_mask;
    graph.passes.pop();
    true
}

/// A hidden final effect is omitted by WE instead of being replaced with a sampling pass. Keep
/// both exact terminal sources in the retained graph so runtime visibility can select the final
/// effect output or the preceding live producer without mutating descriptor topology per frame.
fn append_runtime_terminal_effect_bypass(
    graph: &mut RenderGraph,
    contract: &WeImageGraphContract,
) -> bool {
    let Some(effect) = contract.effect_passes.last() else {
        return false;
    };
    if !effect.runtime_visibility
        || effect.effect_binding_count != 1
        || effect.command.is_some()
        || effect.source.is_some()
        || effect.target.is_some()
        || effect.material_index.is_none()
        || effect.shader.is_none()
        || !effect
            .binds
            .get(&0)
            .is_some_and(|source| matches!(source.as_str(), "previous" | "_previous" | "$previous"))
    {
        return false;
    }
    if !contract.puppet_skinning_after_effects
        && contract.effect_passes[..contract.effect_passes.len() - 1]
            .iter()
            .all(|effect| !effect.runtime_visibility)
        && promote_runtime_terminal_effect_to_scene(graph, effect, contract)
    {
        return true;
    }
    if !contract.effects_in_authored_texture_space {
        return false;
    }
    let Some(terminal_index) = graph.passes.len().checked_sub(1) else {
        return false;
    };
    let Some(effect_index) = terminal_index.checked_sub(1) else {
        return false;
    };
    let Some(previous_index) = effect_index.checked_sub(1) else {
        return false;
    };
    let (previous_target, previous_target_name) = {
        let previous = &graph.passes[previous_index];
        if !matches!(
            previous.target,
            RenderTargetRole::ImageLocalMain | RenderTargetRole::ImageLocalSub
        ) {
            return false;
        }
        (previous.target, previous.target_name.clone())
    };
    let (effect_target, effect_target_name) = {
        let effect_pass = &graph.passes[effect_index];
        if effect_pass.role != RenderPassRole::EffectMaterial
            || effect_pass.effect_visibility.policy
                != super::super::pass::RenderPassEffectVisibilityPolicy::Passthrough
            || !matches!(
                effect_pass.target,
                RenderTargetRole::ImageLocalMain | RenderTargetRole::ImageLocalSub
            )
        {
            return false;
        }
        (effect_pass.target, effect_pass.target_name.clone())
    };
    let terminal = &graph.passes[terminal_index];
    if terminal.role != RenderPassRole::SceneComposite
        || !terminal
            .bindings
            .iter()
            .any(|binding| matches!(binding, TextureBindingRole::PreviousGraphTarget { slot: 0 }))
    {
        return false;
    }

    let visible = RenderPassEffectVisibility::any_visible(
        effect.effect_binding_start,
        effect.effect_binding_count,
    );
    graph.passes[effect_index].effect_visibility = visible;
    graph.passes[terminal_index].effect_visibility = visible;
    replace_previous_target_binding(
        &mut graph.passes[terminal_index],
        effect_target,
        effect_target_name,
    );

    let mut bypass = graph.passes[terminal_index].clone();
    bypass.id = graph.passes.len().min(u32::MAX as usize) as u32;
    bypass.effect_visibility = RenderPassEffectVisibility::none_visible(
        effect.effect_binding_start,
        effect.effect_binding_count,
    );
    replace_previous_target_binding(&mut bypass, previous_target, previous_target_name);
    graph.passes.push(bypass);
    true
}

/// WE omits a hidden effect from the ping-pong chain. A retained passthrough draw is not exact:
/// it adds a resample and also changes which local target every later stage reads and writes.
/// For one independently toggleable internal effect, retain the visible suffix and an exact
/// hidden suffix with the local main/sub route rebuilt from the last stable producer.
fn append_single_internal_runtime_effect_branches(
    graph: &mut RenderGraph,
    contract: &WeImageGraphContract,
) -> bool {
    if !contract.effects_in_authored_texture_space
        || contract.framebuffer_snapshot.is_some()
        || contract.effect_passes.len() < 2
    {
        return false;
    }
    let runtime_effects = contract
        .effect_passes
        .iter()
        .enumerate()
        .filter(|(_, effect)| effect.runtime_visibility)
        .collect::<Vec<_>>();
    let [(runtime_effect_index, runtime_effect)] = runtime_effects.as_slice() else {
        return false;
    };
    if *runtime_effect_index + 1 == contract.effect_passes.len()
        || runtime_effect.effect_binding_count != 1
        || contract.effect_passes.iter().any(|effect| {
            effect.command.is_some()
                || effect.source.is_some()
                || effect.target.is_some()
                || effect.material_index.is_none()
                || effect.shader.is_none()
                || !effect.binds.get(&0).is_some_and(|source| {
                    matches!(source.as_str(), "previous" | "_previous" | "$previous")
                })
        })
    {
        return false;
    }
    let matching_runtime_passes = graph
        .passes
        .iter()
        .enumerate()
        .filter(|(_, pass)| {
            pass.effect_visibility.policy
                == super::super::pass::RenderPassEffectVisibilityPolicy::Passthrough
                && pass.effect_visibility.binding_start == runtime_effect.effect_binding_start
                && pass.effect_visibility.binding_count == runtime_effect.effect_binding_count
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let [runtime_pass_index] = matching_runtime_passes.as_slice() else {
        return false;
    };
    let Some(previous) = runtime_pass_index
        .checked_sub(1)
        .and_then(|index| graph.passes.get(index))
    else {
        return false;
    };
    let previous_target = previous.target;
    if previous.target_name.is_some()
        || !matches!(
            previous.target,
            RenderTargetRole::ImageLocalMain | RenderTargetRole::ImageLocalSub
        )
        || graph.passes[*runtime_pass_index..].iter().any(|pass| {
            pass.target_name.is_some()
                || !matches!(
                    pass.role,
                    RenderPassRole::EffectMaterial | RenderPassRole::SceneComposite
                )
                || !matches!(
                    pass.target,
                    RenderTargetRole::ImageLocalMain
                        | RenderTargetRole::ImageLocalSub
                        | RenderTargetRole::SceneColor
                )
        })
    {
        return false;
    }

    let visible = RenderPassEffectVisibility::any_visible(
        runtime_effect.effect_binding_start,
        runtime_effect.effect_binding_count,
    );
    let hidden = RenderPassEffectVisibility::none_visible(
        runtime_effect.effect_binding_start,
        runtime_effect.effect_binding_count,
    );
    for pass in &mut graph.passes[*runtime_pass_index..] {
        pass.effect_visibility = visible;
    }

    let mut current_target = previous_target;
    let mut hidden_suffix = Vec::with_capacity(graph.passes.len() - *runtime_pass_index - 1);
    for pass in &graph.passes[*runtime_pass_index + 1..] {
        let mut hidden_pass = pass.clone();
        hidden_pass.effect_visibility = hidden;
        replace_previous_target_binding(&mut hidden_pass, current_target, None);
        if matches!(
            hidden_pass.target,
            RenderTargetRole::ImageLocalMain | RenderTargetRole::ImageLocalSub
        ) {
            hidden_pass.target = match current_target {
                RenderTargetRole::ImageLocalMain => RenderTargetRole::ImageLocalSub,
                RenderTargetRole::ImageLocalSub => RenderTargetRole::ImageLocalMain,
                _ => return false,
            };
            current_target = hidden_pass.target;
        }
        hidden_suffix.push(hidden_pass);
    }
    graph.passes.extend(hidden_suffix);
    for (id, pass) in graph.passes.iter_mut().enumerate() {
        pass.id = id.min(u32::MAX as usize) as u32;
    }
    true
}

/// Ordinary unskinned WE image chains do not resample a hidden final effect and then composite it.
/// The last visible material stage itself receives the final SceneColor blend state. Retain the
/// two exact terminal routes so the dynamic final effect can be toggled without rebuilding the
/// graph: the preceding stable stage writes SceneColor when it is hidden, otherwise it writes the
/// local target consumed by the dynamic final stage.
fn promote_runtime_terminal_effect_to_scene(
    graph: &mut RenderGraph,
    effect: &WeEffectPassContract,
    contract: &WeImageGraphContract,
) -> bool {
    if contract.final_scene_blend != SceneBlendMode::Alpha || contract.static_black_output {
        return false;
    }
    let Some(terminal_index) = graph.passes.len().checked_sub(1) else {
        return false;
    };
    let Some(effect_index) = terminal_index.checked_sub(1) else {
        return false;
    };
    let Some(previous_index) = effect_index.checked_sub(1) else {
        return false;
    };
    let terminal = &graph.passes[terminal_index];
    let effect_pass = &graph.passes[effect_index];
    let previous = &graph.passes[previous_index];
    if terminal.role != RenderPassRole::SceneComposite
        || terminal.target != RenderTargetRole::SceneColor
        || effect_pass.role != RenderPassRole::EffectMaterial
        || effect_pass.effect_visibility.policy
            != super::super::pass::RenderPassEffectVisibilityPolicy::Passthrough
        || !matches!(
            effect_pass.target,
            RenderTargetRole::ImageLocalMain | RenderTargetRole::ImageLocalSub
        )
        || effect_pass.target_name.is_some()
        || previous.effect_visibility != RenderPassEffectVisibility::NONE
        || !matches!(
            previous.target,
            RenderTargetRole::ImageLocalMain | RenderTargetRole::ImageLocalSub
        )
        || previous.target_name.is_some()
    {
        return false;
    }

    let previous_source = previous_index.checked_sub(1).map(|source_index| {
        let source = &graph.passes[source_index];
        (source.target, source.target_name.clone())
    });
    let visible = RenderPassEffectVisibility::any_visible(
        effect.effect_binding_start,
        effect.effect_binding_count,
    );
    let hidden = RenderPassEffectVisibility::none_visible(
        effect.effect_binding_start,
        effect.effect_binding_count,
    );
    let terminal_id = terminal.id;
    let terminal_state = terminal.state.clone();
    let previous_is_object_local_source = previous.role == RenderPassRole::ObjectLocalSource;

    graph.passes[previous_index].effect_visibility = visible;
    let final_effect = &mut graph.passes[effect_index];
    final_effect.role = RenderPassRole::SceneComposite;
    final_effect.target = RenderTargetRole::SceneColor;
    if contract.effects_in_authored_texture_space {
        final_effect.draw_primitive = RenderPassDrawPrimitive::ObjectMesh;
    }
    final_effect.effect_visibility = visible;
    final_effect.state.pipeline_blend = terminal_state.pipeline_blend;
    final_effect.state.scene_blend = terminal_state.scene_blend;
    final_effect.state.color_write_mask = terminal_state.color_write_mask;

    let mut previous_terminal = if previous_is_object_local_source {
        RenderPassNode {
            id: terminal_id,
            role: RenderPassRole::BaseMaterial,
            draw_primitive: RenderPassDrawPrimitive::ObjectMesh,
            object_index: Some(contract.object_index),
            material_index: contract.base_material_index,
            pass_index: 0,
            shader: Some(direct_base_shader(contract)),
            target: RenderTargetRole::SceneColor,
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
                .collect(),
            effect_visibility: hidden,
            state: terminal_state.clone(),
        }
    } else {
        graph.passes[previous_index].clone()
    };
    previous_terminal.id = terminal_id;
    if !previous_is_object_local_source {
        previous_terminal.role = RenderPassRole::SceneComposite;
    }
    previous_terminal.target = RenderTargetRole::SceneColor;
    if contract.effects_in_authored_texture_space {
        previous_terminal.draw_primitive = RenderPassDrawPrimitive::ObjectMesh;
    }
    previous_terminal.effect_visibility = hidden;
    previous_terminal.state.pipeline_blend = terminal_state.pipeline_blend;
    previous_terminal.state.scene_blend = terminal_state.scene_blend;
    previous_terminal.state.color_write_mask = terminal_state.color_write_mask;
    if let Some((source_role, source_name)) = previous_source {
        replace_previous_target_binding(&mut previous_terminal, source_role, source_name);
    }

    graph.passes.pop();
    graph.passes.push(previous_terminal);
    true
}

fn replace_previous_target_binding(
    pass: &mut RenderPassNode,
    role: RenderTargetRole,
    name: Option<String>,
) {
    for binding in &mut pass.bindings {
        match binding {
            TextureBindingRole::PreviousGraphTarget { slot }
            | TextureBindingRole::GraphTarget { slot, .. }
                if *slot == 0 =>
            {
                *binding = TextureBindingRole::GraphTarget {
                    slot: *slot,
                    role,
                    name: name.clone(),
                };
            }
            _ => {}
        }
    }
}
