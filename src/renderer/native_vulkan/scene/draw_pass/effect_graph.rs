use crate::core::SceneBlendMode;
use crate::core::scene::scene_blend_mode_from_material_blending;

use super::blend::native_vulkan_scene_render_state;
use super::{
    NativeVulkanSceneCullMode, NativeVulkanSceneEffectKind, NativeVulkanSceneMaterialFlag,
    NativeVulkanSceneRenderState, NativeVulkanSceneSampledImageQuad,
    NativeVulkanSceneWeImageGraphPlan, NativeVulkanSceneWeImageGraphStep,
    NativeVulkanSceneWeImageGraphTarget, NativeVulkanSceneWeImageGraphTextureBinding,
    NativeVulkanSceneWeImageGraphTextureBindingSource, NativeVulkanSceneWeImagePass,
    NativeVulkanSceneWeImagePassChain, NativeVulkanSceneWeImagePassEndpoint,
    NativeVulkanSceneWeImagePassExecution, NativeVulkanSceneWeImagePassRole,
};

pub(in crate::renderer::native_vulkan::scene) fn native_vulkan_scene_we_image_graph_plan(
    quads: &[NativeVulkanSceneSampledImageQuad],
) -> NativeVulkanSceneWeImageGraphPlan {
    let mut plan = NativeVulkanSceneWeImageGraphPlan::default();
    for quad in quads {
        let Some(chain) = native_vulkan_scene_we_image_pass_chain(quad) else {
            continue;
        };
        let chain_index = plan.chain_count;
        plan.chain_count += 1;
        match chain.execution {
            NativeVulkanSceneWeImagePassExecution::FirstClassTarget => {
                plan.first_class_target_chain_count += 1;
            }
            NativeVulkanSceneWeImagePassExecution::TemporaryRawFallback => {
                plan.temporary_raw_fallback_chain_count += 1;
            }
            NativeVulkanSceneWeImagePassExecution::SuppressedUntilGraphExecutor => {
                plan.suppressed_chain_count += 1;
            }
            NativeVulkanSceneWeImagePassExecution::Direct => {}
        }
        let chain_targets = native_vulkan_scene_we_image_graph_targets(
            quad,
            chain_index,
            &chain,
            plan.targets.len(),
        );
        plan.targets.extend(chain_targets.iter().cloned());
        plan.final_scene_step_count += chain
            .passes
            .iter()
            .filter(|pass| pass.final_scene_pass)
            .count();
        for (step_index, pass) in chain.passes.into_iter().enumerate() {
            if let Some(effect_kind) = pass.effect_kind {
                *plan
                    .effect_kind_counts
                    .entry(effect_kind.as_str())
                    .or_default() += 1;
            }
            let input_target_index = native_vulkan_scene_we_image_graph_target_index(
                &chain_targets,
                pass.input,
                pass.input_name.as_deref(),
            );
            let output_target_index = native_vulkan_scene_we_image_graph_target_index(
                &chain_targets,
                pass.target,
                pass.target_name.as_deref(),
            );
            let texture_bindings = native_vulkan_scene_we_image_graph_texture_bindings(
                quad,
                &chain_targets,
                &pass,
                input_target_index,
            );
            plan.steps.push(NativeVulkanSceneWeImageGraphStep {
                layer_index: quad.layer_index,
                layer_id: quad.layer_id.clone(),
                chain_index,
                step_index,
                execution: chain.execution,
                raw_direct_composite_allowed: chain.raw_direct_composite_allowed,
                unsupported_reason: chain.unsupported_reason,
                input_target_index,
                output_target_index,
                texture_bindings,
                pass,
            });
        }
    }
    plan.target_count = plan.targets.len();
    plan.step_count = plan.steps.len();
    plan
}

pub(in crate::renderer::native_vulkan::scene) fn native_vulkan_scene_we_image_pass_chain(
    quad: &NativeVulkanSceneSampledImageQuad,
) -> Option<NativeVulkanSceneWeImagePassChain> {
    let color_blend_passthrough =
        native_vulkan_scene_we_image_pass_chain_uses_color_blend_passthrough(quad.base_blend_mode);
    let first_class_target = quad.effect_target_pass.is_some();
    let local_target_required =
        first_class_target || color_blend_passthrough || !quad.effect_passes.is_empty();
    if !local_target_required {
        return None;
    }

    let raw_direct_composite_allowed =
        native_vulkan_scene_we_image_pass_chain_allows_temporary_raw_composite(quad);
    let execution = if first_class_target {
        NativeVulkanSceneWeImagePassExecution::FirstClassTarget
    } else if raw_direct_composite_allowed {
        NativeVulkanSceneWeImagePassExecution::TemporaryRawFallback
    } else {
        NativeVulkanSceneWeImagePassExecution::SuppressedUntilGraphExecutor
    };
    let unsupported_reason =
        (!raw_direct_composite_allowed).then_some("we-effect-graph-passthrough-water-not-executed");
    let logical_pass_count = 1 + quad.effect_passes.len() + usize::from(color_blend_passthrough);
    let first_pass_blend_moved_to_final = logical_pass_count > 1;
    let ping_pong_required = logical_pass_count > 2;
    let mut passes = Vec::with_capacity(logical_pass_count);

    let base_target = if first_class_target {
        NativeVulkanSceneWeImagePassEndpoint::FirstClassEffectTarget
    } else if first_pass_blend_moved_to_final {
        NativeVulkanSceneWeImagePassEndpoint::ImageLocalMain
    } else {
        NativeVulkanSceneWeImagePassEndpoint::Scene
    };
    let base_blend_mode = if first_pass_blend_moved_to_final {
        SceneBlendMode::Normal
    } else {
        quad.base_blend_mode
    };
    let base_depth_test = quad.material_pass.render_state.depth_test;
    let base_depth_write = quad.material_pass.render_state.depth_write;
    let base_cull_mode = quad.material_pass.render_state.cull_mode.clone();
    passes.push(NativeVulkanSceneWeImagePass {
        pass_index: 0,
        role: NativeVulkanSceneWeImagePassRole::BaseMaterial,
        effect_kind: None,
        effect_file: None,
        command: None,
        source: None,
        target_name: None,
        binds: Default::default(),
        fbos: Default::default(),
        shader: quad.material_pass.shader.clone(),
        blending: quad.material_pass.blending.clone(),
        scene_blend_mode: base_blend_mode,
        render_state: native_vulkan_scene_we_image_pass_render_state(
            base_blend_mode,
            base_depth_test,
            base_depth_write,
            &base_cull_mode,
        ),
        input: NativeVulkanSceneWeImagePassEndpoint::SourceTexture,
        input_name: None,
        target: base_target,
        final_scene_pass: base_target == NativeVulkanSceneWeImagePassEndpoint::Scene,
        texture_slots: quad.texture_slots.clone(),
        texture_slot_count: quad.texture_slots.len(),
        parameter_keys: Vec::new(),
        combo_keys: quad.material_pass.combo_keys.clone(),
        depth_test: base_depth_test,
        depth_write: base_depth_write,
        cull_mode: base_cull_mode,
    });

    let mut previous_output = base_target;
    let mut previous_output_name = None::<String>;
    for (effect_index, effect) in quad.effect_passes.iter().enumerate() {
        let has_following_effect = effect_index + 1 < quad.effect_passes.len();
        let explicit_target_name = effect.target.clone();
        let final_scene_pass =
            explicit_target_name.is_none() && !color_blend_passthrough && !has_following_effect;
        let target = if explicit_target_name.is_some() {
            NativeVulkanSceneWeImagePassEndpoint::NamedFbo
        } else if final_scene_pass {
            NativeVulkanSceneWeImagePassEndpoint::Scene
        } else if effect_index % 2 == 0 {
            NativeVulkanSceneWeImagePassEndpoint::ImageLocalSub
        } else {
            NativeVulkanSceneWeImagePassEndpoint::ImageLocalMain
        };
        let scene_blend_mode = if final_scene_pass {
            quad.base_blend_mode
        } else {
            effect
                .blending
                .as_deref()
                .and_then(scene_blend_mode_from_material_blending)
                .unwrap_or(SceneBlendMode::Normal)
        };
        let depth_test = effect.depth_test;
        let depth_write = effect.depth_write;
        let cull_mode = effect.cull_mode.clone();
        passes.push(NativeVulkanSceneWeImagePass {
            pass_index: effect.pass_index,
            role: NativeVulkanSceneWeImagePassRole::EffectMaterial,
            effect_kind: Some(effect.kind),
            effect_file: Some(effect.effect_file.clone()),
            command: effect.command.clone(),
            source: effect.source.clone(),
            target_name: effect.target.clone(),
            binds: effect.binds.clone(),
            fbos: effect.fbos.clone(),
            shader: effect.shader.clone(),
            blending: effect.blending.clone(),
            scene_blend_mode,
            render_state: native_vulkan_scene_we_image_pass_render_state(
                scene_blend_mode,
                depth_test,
                depth_write,
                &cull_mode,
            ),
            input: previous_output,
            input_name: previous_output_name.clone(),
            target,
            final_scene_pass,
            texture_slots: effect.texture_slots.clone(),
            texture_slot_count: effect.texture_slots.len(),
            parameter_keys: effect.parameter_keys.clone(),
            combo_keys: effect.combo_keys.clone(),
            depth_test,
            depth_write,
            cull_mode,
        });
        previous_output = target;
        previous_output_name = (target == NativeVulkanSceneWeImagePassEndpoint::NamedFbo)
            .then(|| explicit_target_name.clone())
            .flatten();
    }

    if color_blend_passthrough {
        let passthrough_depth_test = NativeVulkanSceneMaterialFlag::Disabled;
        let passthrough_depth_write = NativeVulkanSceneMaterialFlag::Disabled;
        let passthrough_cull_mode = quad.material_pass.render_state.cull_mode.clone();
        passes.push(NativeVulkanSceneWeImagePass {
            pass_index: passes.len(),
            role: NativeVulkanSceneWeImagePassRole::ColorBlendPassthrough,
            effect_kind: None,
            effect_file: Some("materials/util/effectpassthrough.json".to_owned()),
            command: None,
            source: None,
            target_name: None,
            binds: Default::default(),
            fbos: Default::default(),
            shader: Some("util/effectpassthrough".to_owned()),
            blending: Some("normal".to_owned()),
            scene_blend_mode: quad.base_blend_mode,
            render_state: native_vulkan_scene_we_image_pass_render_state(
                quad.base_blend_mode,
                passthrough_depth_test,
                passthrough_depth_write,
                &passthrough_cull_mode,
            ),
            input: previous_output,
            input_name: previous_output_name,
            target: NativeVulkanSceneWeImagePassEndpoint::Scene,
            final_scene_pass: true,
            texture_slots: Vec::new(),
            texture_slot_count: 1,
            parameter_keys: Vec::new(),
            combo_keys: Vec::new(),
            depth_test: passthrough_depth_test,
            depth_write: passthrough_depth_write,
            cull_mode: passthrough_cull_mode,
        });
    }

    Some(NativeVulkanSceneWeImagePassChain {
        execution,
        local_target_required,
        ping_pong_required,
        first_pass_blend_moved_to_final,
        color_blend_passthrough,
        final_scene_blend_mode: quad.base_blend_mode,
        raw_direct_composite_allowed,
        unsupported_reason,
        passes,
    })
}

fn native_vulkan_scene_we_image_pass_render_state(
    blend_mode: SceneBlendMode,
    depth_test: NativeVulkanSceneMaterialFlag,
    depth_write: NativeVulkanSceneMaterialFlag,
    cull_mode: &NativeVulkanSceneCullMode,
) -> NativeVulkanSceneRenderState {
    native_vulkan_scene_render_state(blend_mode, depth_test, depth_write, cull_mode.clone())
}

fn native_vulkan_scene_we_image_pass_chain_uses_color_blend_passthrough(
    blend_mode: SceneBlendMode,
) -> bool {
    !matches!(blend_mode, SceneBlendMode::Alpha | SceneBlendMode::Normal)
}

fn native_vulkan_scene_we_image_pass_chain_allows_temporary_raw_composite(
    quad: &NativeVulkanSceneSampledImageQuad,
) -> bool {
    if quad.effect_target_pass.is_some() {
        return true;
    }
    !native_vulkan_scene_we_image_pass_chain_uses_color_blend_passthrough(quad.base_blend_mode)
        || !quad.effect_passes.iter().any(|pass| {
            matches!(
                pass.kind,
                NativeVulkanSceneEffectKind::WaterRipple
                    | NativeVulkanSceneEffectKind::WaterFlow
                    | NativeVulkanSceneEffectKind::WaterCaustics
            )
        })
}

fn native_vulkan_scene_we_image_graph_targets(
    quad: &NativeVulkanSceneSampledImageQuad,
    chain_index: usize,
    chain: &NativeVulkanSceneWeImagePassChain,
    first_target_index: usize,
) -> Vec<NativeVulkanSceneWeImageGraphTarget> {
    let mut targets = Vec::new();
    for pass in &chain.passes {
        let endpoint = pass.target;
        let target_name = (endpoint == NativeVulkanSceneWeImagePassEndpoint::NamedFbo)
            .then(|| pass.target_name.clone())
            .flatten();
        if !endpoint.is_graph_target()
            || targets
                .iter()
                .any(|target: &NativeVulkanSceneWeImageGraphTarget| {
                    target.endpoint == endpoint && target.name == target_name
                })
        {
            continue;
        }
        let first_write_step_index = chain
            .passes
            .iter()
            .position(|candidate| {
                candidate.target == endpoint
                    && native_vulkan_scene_we_image_graph_endpoint_name(candidate)
                        == target_name.as_deref()
            })
            .unwrap_or(pass.pass_index);
        let write_count = chain
            .passes
            .iter()
            .filter(|candidate| {
                candidate.target == endpoint
                    && native_vulkan_scene_we_image_graph_endpoint_name(candidate)
                        == target_name.as_deref()
            })
            .count();
        let sampled_by_following_pass = chain
            .passes
            .iter()
            .skip(first_write_step_index.saturating_add(1))
            .any(|candidate| {
                (candidate.input == endpoint
                    && candidate.input_name.as_deref() == target_name.as_deref())
                    || target_name
                        .as_ref()
                        .is_some_and(|name| candidate.binds.values().any(|bind| bind == name))
            });
        let scene_composite_source = chain.passes.iter().any(|candidate| {
            candidate.final_scene_pass
                && candidate.input == endpoint
                && candidate.input_name.as_deref() == target_name.as_deref()
        });
        let fbo = target_name.as_ref().and_then(|name| {
            pass.fbos.iter().find(|fbo| fbo.name == *name).or_else(|| {
                chain
                    .passes
                    .iter()
                    .flat_map(|pass| &pass.fbos)
                    .find(|fbo| fbo.name == *name)
            })
        });
        let scale = fbo.map(|fbo| fbo.scale);
        targets.push(NativeVulkanSceneWeImageGraphTarget {
            layer_index: quad.layer_index,
            layer_id: quad.layer_id.clone(),
            chain_index,
            target_index: first_target_index
                .saturating_add(targets.len())
                .min(u32::MAX as usize) as u32,
            endpoint,
            name: target_name,
            format: fbo.and_then(|fbo| fbo.format.clone()),
            scale,
            unique: fbo.is_some_and(|fbo| fbo.unique),
            execution: chain.execution,
            width: native_vulkan_scene_we_image_graph_scaled_target_extent(quad.width, scale),
            height: native_vulkan_scene_we_image_graph_scaled_target_extent(quad.height, scale),
            first_write_step_index,
            write_count,
            sampled_by_following_pass,
            scene_composite_source,
            clear_before_first_write: true,
        });
    }
    for pass in &chain.passes {
        for fbo in &pass.fbos {
            if targets.iter().any(|target| {
                target.endpoint == NativeVulkanSceneWeImagePassEndpoint::NamedFbo
                    && target.name.as_deref() == Some(fbo.name.as_str())
            }) {
                continue;
            }
            let sampled_by_following_pass = chain
                .passes
                .iter()
                .any(|candidate| candidate.binds.values().any(|bind| bind == &fbo.name));
            targets.push(NativeVulkanSceneWeImageGraphTarget {
                layer_index: quad.layer_index,
                layer_id: quad.layer_id.clone(),
                chain_index,
                target_index: first_target_index
                    .saturating_add(targets.len())
                    .min(u32::MAX as usize) as u32,
                endpoint: NativeVulkanSceneWeImagePassEndpoint::NamedFbo,
                name: Some(fbo.name.clone()),
                format: fbo.format.clone(),
                scale: Some(fbo.scale),
                unique: fbo.unique,
                execution: chain.execution,
                width: native_vulkan_scene_we_image_graph_scaled_target_extent(
                    quad.width,
                    Some(fbo.scale),
                ),
                height: native_vulkan_scene_we_image_graph_scaled_target_extent(
                    quad.height,
                    Some(fbo.scale),
                ),
                first_write_step_index: chain.passes.len(),
                write_count: 0,
                sampled_by_following_pass,
                scene_composite_source: false,
                clear_before_first_write: true,
            });
        }
    }
    targets
}

fn native_vulkan_scene_we_image_graph_target_index(
    targets: &[NativeVulkanSceneWeImageGraphTarget],
    endpoint: NativeVulkanSceneWeImagePassEndpoint,
    name: Option<&str>,
) -> Option<u32> {
    if !endpoint.is_graph_target() {
        return None;
    }
    targets
        .iter()
        .find(|target| target.endpoint == endpoint && target.name.as_deref() == name)
        .map(|target| target.target_index)
}

fn native_vulkan_scene_we_image_graph_endpoint_name(
    pass: &NativeVulkanSceneWeImagePass,
) -> Option<&str> {
    (pass.target == NativeVulkanSceneWeImagePassEndpoint::NamedFbo)
        .then_some(pass.target_name.as_deref())
        .flatten()
}

fn native_vulkan_scene_we_image_graph_texture_bindings(
    quad: &NativeVulkanSceneSampledImageQuad,
    targets: &[NativeVulkanSceneWeImageGraphTarget],
    pass: &NativeVulkanSceneWeImagePass,
    input_target_index: Option<u32>,
) -> Vec<NativeVulkanSceneWeImageGraphTextureBinding> {
    let mut bindings = Vec::new();
    match pass.role {
        NativeVulkanSceneWeImagePassRole::BaseMaterial => {
            native_vulkan_scene_we_image_graph_push_source_texture_binding(&mut bindings, quad);
        }
        NativeVulkanSceneWeImagePassRole::EffectMaterial
        | NativeVulkanSceneWeImagePassRole::ColorBlendPassthrough => {
            native_vulkan_scene_we_image_graph_push_input_texture_binding(
                &mut bindings,
                quad,
                targets,
                pass.input,
                pass.input_name.as_deref(),
                input_target_index,
                pass.binds.get(&0),
            );
        }
    }

    if pass.role == NativeVulkanSceneWeImagePassRole::EffectMaterial {
        for slot in &pass.texture_slots {
            if slot.slot == 0 {
                continue;
            }
            if let Some(bind_name) = pass.binds.get(&slot.slot) {
                native_vulkan_scene_we_image_graph_push_bound_texture_binding(
                    &mut bindings,
                    targets,
                    pass.input,
                    pass.input_name.as_deref(),
                    input_target_index,
                    slot.slot,
                    bind_name,
                );
            } else {
                bindings.push(NativeVulkanSceneWeImageGraphTextureBinding {
                    slot: slot.slot,
                    uniform: native_vulkan_scene_we_image_graph_texture_uniform(slot.slot),
                    source: NativeVulkanSceneWeImageGraphTextureBindingSource::PassTextureSlot,
                    target_index: None,
                    endpoint: None,
                    bind_name: None,
                    source_path: Some(slot.source.clone()),
                    width: slot.width,
                    height: slot.height,
                    resolution: native_vulkan_scene_we_image_graph_texture_resolution(
                        slot.width,
                        slot.height,
                    ),
                });
            }
        }
        for (slot, bind_name) in &pass.binds {
            if *slot == 0 || bindings.iter().any(|binding| binding.slot == *slot) {
                continue;
            }
            native_vulkan_scene_we_image_graph_push_bound_texture_binding(
                &mut bindings,
                targets,
                pass.input,
                pass.input_name.as_deref(),
                input_target_index,
                *slot,
                bind_name,
            );
        }
    }
    bindings.sort_by_key(|binding| binding.slot);
    bindings
}

fn native_vulkan_scene_we_image_graph_push_source_texture_binding(
    bindings: &mut Vec<NativeVulkanSceneWeImageGraphTextureBinding>,
    quad: &NativeVulkanSceneSampledImageQuad,
) {
    let source_slot = quad.texture_slots.iter().find(|slot| slot.slot == 0);
    let width = source_slot
        .and_then(|slot| slot.width)
        .or_else(|| native_vulkan_scene_we_image_graph_extent_from_f64(quad.width));
    let height = source_slot
        .and_then(|slot| slot.height)
        .or_else(|| native_vulkan_scene_we_image_graph_extent_from_f64(quad.height));
    bindings.push(NativeVulkanSceneWeImageGraphTextureBinding {
        slot: 0,
        uniform: native_vulkan_scene_we_image_graph_texture_uniform(0),
        source: NativeVulkanSceneWeImageGraphTextureBindingSource::SourceTexture,
        target_index: None,
        endpoint: Some(NativeVulkanSceneWeImagePassEndpoint::SourceTexture),
        bind_name: None,
        source_path: Some(quad.source.clone()),
        width,
        height,
        resolution: native_vulkan_scene_we_image_graph_texture_resolution(width, height),
    });
}

fn native_vulkan_scene_we_image_graph_push_input_texture_binding(
    bindings: &mut Vec<NativeVulkanSceneWeImageGraphTextureBinding>,
    quad: &NativeVulkanSceneSampledImageQuad,
    targets: &[NativeVulkanSceneWeImageGraphTarget],
    endpoint: NativeVulkanSceneWeImagePassEndpoint,
    endpoint_name: Option<&str>,
    input_target_index: Option<u32>,
    bind_name: Option<&String>,
) {
    if let Some(bind_name) = bind_name {
        native_vulkan_scene_we_image_graph_push_bound_texture_binding(
            bindings,
            targets,
            endpoint,
            endpoint_name,
            input_target_index,
            0,
            bind_name,
        );
        return;
    }
    if endpoint == NativeVulkanSceneWeImagePassEndpoint::SourceTexture {
        native_vulkan_scene_we_image_graph_push_source_texture_binding(bindings, quad);
        return;
    }

    let target = input_target_index.and_then(|target_index| {
        targets.iter().find(|target| {
            target.target_index == target_index && target.name.as_deref() == endpoint_name
        })
    });
    bindings.push(NativeVulkanSceneWeImageGraphTextureBinding {
        slot: 0,
        uniform: native_vulkan_scene_we_image_graph_texture_uniform(0),
        source: NativeVulkanSceneWeImageGraphTextureBindingSource::PreviousGraphTarget,
        target_index: input_target_index,
        endpoint: Some(endpoint),
        bind_name: None,
        source_path: None,
        width: target.map(|target| target.width),
        height: target.map(|target| target.height),
        resolution: target.map(|target| [target.width, target.height]),
    });
}

fn native_vulkan_scene_we_image_graph_push_bound_texture_binding(
    bindings: &mut Vec<NativeVulkanSceneWeImageGraphTextureBinding>,
    targets: &[NativeVulkanSceneWeImageGraphTarget],
    endpoint: NativeVulkanSceneWeImagePassEndpoint,
    endpoint_name: Option<&str>,
    input_target_index: Option<u32>,
    slot: u32,
    bind_name: &str,
) {
    if bind_name == "previous" {
        let target = input_target_index.and_then(|target_index| {
            targets.iter().find(|target| {
                target.target_index == target_index && target.name.as_deref() == endpoint_name
            })
        });
        bindings.push(NativeVulkanSceneWeImageGraphTextureBinding {
            slot,
            uniform: native_vulkan_scene_we_image_graph_texture_uniform(slot),
            source: NativeVulkanSceneWeImageGraphTextureBindingSource::PreviousGraphTarget,
            target_index: input_target_index,
            endpoint: Some(endpoint),
            bind_name: Some(bind_name.to_owned()),
            source_path: None,
            width: target.map(|target| target.width),
            height: target.map(|target| target.height),
            resolution: target.map(|target| [target.width, target.height]),
        });
        return;
    }
    let target = targets.iter().find(|target| {
        target.endpoint == NativeVulkanSceneWeImagePassEndpoint::NamedFbo
            && target.name.as_deref() == Some(bind_name)
    });
    bindings.push(NativeVulkanSceneWeImageGraphTextureBinding {
        slot,
        uniform: native_vulkan_scene_we_image_graph_texture_uniform(slot),
        source: NativeVulkanSceneWeImageGraphTextureBindingSource::NamedFboBind,
        target_index: target.map(|target| target.target_index),
        endpoint: Some(NativeVulkanSceneWeImagePassEndpoint::NamedFbo).filter(|_| target.is_some()),
        bind_name: Some(bind_name.to_owned()),
        source_path: None,
        width: target.map(|target| target.width),
        height: target.map(|target| target.height),
        resolution: target.map(|target| [target.width, target.height]),
    });
}

fn native_vulkan_scene_we_image_graph_texture_uniform(slot: u32) -> String {
    format!("g_Texture{slot}")
}

fn native_vulkan_scene_we_image_graph_texture_resolution(
    width: Option<u32>,
    height: Option<u32>,
) -> Option<[u32; 2]> {
    Some([width?, height?])
}

fn native_vulkan_scene_we_image_graph_extent_from_f64(value: f64) -> Option<u32> {
    if !value.is_finite() || value <= 0.0 {
        return None;
    }
    Some(value.ceil().clamp(1.0, u32::MAX as f64) as u32)
}

fn native_vulkan_scene_we_image_graph_target_extent(value: f64) -> u32 {
    if !value.is_finite() || value <= 0.0 {
        return 1;
    }
    value.ceil().clamp(1.0, u32::MAX as f64) as u32
}

fn native_vulkan_scene_we_image_graph_scaled_target_extent(value: f64, scale: Option<f64>) -> u32 {
    let scale = scale
        .filter(|scale| scale.is_finite() && *scale > 0.0)
        .unwrap_or(1.0);
    native_vulkan_scene_we_image_graph_target_extent(value * scale)
}
