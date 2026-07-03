use crate::core::SceneBlendMode;
use crate::core::scene::{SceneMesh, scene_blend_mode_from_material_blending};

use super::blend::native_vulkan_scene_render_state;
use super::{
    NativeVulkanSceneCullMode, NativeVulkanSceneEffectKind, NativeVulkanSceneEffectRecord,
    NativeVulkanSceneMaterialFlag, NativeVulkanSceneRenderState, NativeVulkanSceneSampledImageQuad,
    NativeVulkanSceneWeImageGraphPlan, NativeVulkanSceneWeImageGraphStep,
    NativeVulkanSceneWeImageGraphTarget, NativeVulkanSceneWeImageGraphTargetBounds,
    NativeVulkanSceneWeImageGraphTextureBinding, NativeVulkanSceneWeImageGraphTextureBindingSource,
    NativeVulkanSceneWeImagePass, NativeVulkanSceneWeImagePassChain,
    NativeVulkanSceneWeImagePassEndpoint, NativeVulkanSceneWeImagePassExecution,
    NativeVulkanSceneWeImagePassRole,
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
    let first_class_target = quad.effect_target_pass.is_some();
    let has_effect_passes = !quad.effect_passes.is_empty();
    let color_blend_passthrough =
        native_vulkan_scene_we_image_pass_chain_uses_color_blend_passthrough(quad.base_blend_mode)
            && (first_class_target || has_effect_passes);
    let local_target_required = first_class_target || color_blend_passthrough || has_effect_passes;
    if !local_target_required {
        return None;
    }

    let material_graph_supported =
        native_vulkan_scene_we_image_pass_chain_has_executable_material_graph(quad);
    let raw_direct_composite_allowed =
        native_vulkan_scene_we_image_pass_chain_allows_temporary_raw_composite(quad);
    let color_blend_passthrough_folded = color_blend_passthrough
        && native_vulkan_scene_we_image_pass_chain_can_fold_color_blend_passthrough(quad);
    let execution = if first_class_target || material_graph_supported {
        NativeVulkanSceneWeImagePassExecution::FirstClassTarget
    } else if raw_direct_composite_allowed {
        NativeVulkanSceneWeImagePassExecution::TemporaryRawFallback
    } else {
        NativeVulkanSceneWeImagePassExecution::SuppressedUntilGraphExecutor
    };
    let unsupported_reason = (!raw_direct_composite_allowed && !material_graph_supported)
        .then_some("we-effect-graph-material-pass-not-executed");
    if native_vulkan_scene_we_image_pass_chain_can_direct_terminal_effect(
        quad,
        first_class_target,
        color_blend_passthrough,
        material_graph_supported,
    ) {
        let effect = &quad.effect_passes[0];
        let final_scene_blend_mode =
            native_vulkan_scene_we_final_scene_blend_mode(quad.base_blend_mode);
        return Some(NativeVulkanSceneWeImagePassChain {
            execution,
            local_target_required: false,
            ping_pong_required: false,
            first_pass_blend_moved_to_final: false,
            color_blend_passthrough: false,
            final_scene_blend_mode,
            raw_direct_composite_allowed,
            unsupported_reason,
            passes: vec![NativeVulkanSceneWeImagePass {
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
                scene_blend_mode: final_scene_blend_mode,
                render_state: native_vulkan_scene_we_image_pass_render_state(
                    final_scene_blend_mode,
                    effect.depth_test,
                    effect.depth_write,
                    &effect.cull_mode,
                ),
                input: NativeVulkanSceneWeImagePassEndpoint::SourceTexture,
                input_name: None,
                target: NativeVulkanSceneWeImagePassEndpoint::Scene,
                final_scene_pass: true,
                texture_slots: effect.texture_slots.clone(),
                texture_slot_count: effect.texture_slots.len(),
                effect_uv_transform: effect.effect_uv_transform,
                parameter_keys: effect.parameter_keys.clone(),
                constant_shader_values: effect.constant_shader_values.clone(),
                combo_keys: effect.combo_keys.clone(),
                combo_values: effect.combo_values.clone(),
                depth_test: effect.depth_test,
                depth_write: effect.depth_write,
                cull_mode: effect.cull_mode.clone(),
            }],
        });
    }
    let color_blend_passthrough_pass = color_blend_passthrough && !color_blend_passthrough_folded;
    let logical_pass_count =
        1 + quad.effect_passes.len() + usize::from(color_blend_passthrough_pass);
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
    let base_blend_mode = if first_pass_blend_moved_to_final && quad.mesh.is_some() {
        // CWE reference: CImage::setupPasses() forces the first pass that
        // draws puppet geometry to BlendingMode_Translucent even after
        // CImage::setup() moves the image blend mode to the final pass.
        SceneBlendMode::Alpha
    } else if first_pass_blend_moved_to_final {
        SceneBlendMode::Normal
    } else {
        quad.base_blend_mode
    };
    let base_depth_test = NativeVulkanSceneMaterialFlag::Unspecified;
    let base_depth_write = NativeVulkanSceneMaterialFlag::Unspecified;
    let base_cull_mode = NativeVulkanSceneCullMode::Unspecified;
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
        shader: None,
        blending: None,
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
        effect_uv_transform: None,
        parameter_keys: Vec::new(),
        constant_shader_values: Default::default(),
        combo_keys: Vec::new(),
        combo_values: Default::default(),
        depth_test: base_depth_test,
        depth_write: base_depth_write,
        cull_mode: base_cull_mode,
    });

    let mut previous_output = base_target;
    let mut previous_output_name = None::<String>;
    for (effect_index, effect) in quad.effect_passes.iter().enumerate() {
        let has_following_effect = effect_index + 1 < quad.effect_passes.len();
        let explicit_target_name = effect.target.clone();
        let final_scene_pass = explicit_target_name.is_none()
            && !has_following_effect
            && (!color_blend_passthrough || color_blend_passthrough_folded);
        let target = if explicit_target_name.is_some() {
            NativeVulkanSceneWeImagePassEndpoint::NamedFbo
        } else if final_scene_pass {
            NativeVulkanSceneWeImagePassEndpoint::Scene
        } else if effect_index % 2 == 0 {
            NativeVulkanSceneWeImagePassEndpoint::ImageLocalSub
        } else {
            NativeVulkanSceneWeImagePassEndpoint::ImageLocalMain
        };
        let effect_blend_mode = native_vulkan_scene_we_effect_pass_blend_mode(effect);
        let scene_blend_mode = if final_scene_pass {
            native_vulkan_scene_we_final_scene_blend_mode(quad.base_blend_mode)
        } else {
            effect_blend_mode
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
            effect_uv_transform: effect.effect_uv_transform,
            parameter_keys: effect.parameter_keys.clone(),
            constant_shader_values: effect.constant_shader_values.clone(),
            combo_keys: effect.combo_keys.clone(),
            combo_values: effect.combo_values.clone(),
            depth_test,
            depth_write,
            cull_mode,
        });
        previous_output = target;
        previous_output_name = (target == NativeVulkanSceneWeImagePassEndpoint::NamedFbo)
            .then(|| explicit_target_name.clone())
            .flatten();
    }

    if color_blend_passthrough_pass {
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
            effect_uv_transform: None,
            parameter_keys: Vec::new(),
            constant_shader_values: Default::default(),
            combo_keys: Vec::new(),
            combo_values: Default::default(),
            depth_test: passthrough_depth_test,
            depth_write: passthrough_depth_write,
            cull_mode: passthrough_cull_mode,
        });
    }

    let final_scene_blend_mode =
        native_vulkan_scene_we_image_pass_chain_final_blend_mode(&passes, quad.base_blend_mode);

    Some(NativeVulkanSceneWeImagePassChain {
        execution,
        local_target_required,
        ping_pong_required,
        first_pass_blend_moved_to_final,
        color_blend_passthrough: color_blend_passthrough_pass,
        final_scene_blend_mode,
        raw_direct_composite_allowed,
        unsupported_reason,
        passes,
    })
}

fn native_vulkan_scene_we_effect_pass_blend_mode(
    effect: &NativeVulkanSceneEffectRecord,
) -> SceneBlendMode {
    effect
        .blending
        .as_deref()
        .and_then(scene_blend_mode_from_material_blending)
        .unwrap_or(SceneBlendMode::Normal)
}

fn native_vulkan_scene_we_image_pass_chain_final_blend_mode(
    passes: &[NativeVulkanSceneWeImagePass],
    fallback: SceneBlendMode,
) -> SceneBlendMode {
    passes
        .iter()
        .rev()
        .find(|pass| pass.final_scene_pass)
        .map(|pass| pass.scene_blend_mode)
        .unwrap_or(fallback)
}

fn native_vulkan_scene_we_final_scene_blend_mode(
    base_blend_mode: SceneBlendMode,
) -> SceneBlendMode {
    if matches!(base_blend_mode, SceneBlendMode::Normal) {
        SceneBlendMode::Alpha
    } else {
        base_blend_mode
    }
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
    !quad.effect_passes.iter().any(|pass| {
        matches!(
            pass.kind,
            NativeVulkanSceneEffectKind::WaterRipple
                | NativeVulkanSceneEffectKind::WaterFlow
                | NativeVulkanSceneEffectKind::WaterCaustics
                | NativeVulkanSceneEffectKind::ColorKey
                | NativeVulkanSceneEffectKind::ClippingMask
        )
    })
}

fn native_vulkan_scene_we_image_pass_chain_has_executable_material_graph(
    quad: &NativeVulkanSceneSampledImageQuad,
) -> bool {
    !quad.effect_passes.is_empty()
        && quad
            .effect_passes
            .iter()
            .all(native_vulkan_scene_effect_pass_has_executable_material_graph)
}

fn native_vulkan_scene_we_image_pass_chain_can_direct_terminal_effect(
    quad: &NativeVulkanSceneSampledImageQuad,
    first_class_target: bool,
    color_blend_passthrough: bool,
    material_graph_supported: bool,
) -> bool {
    if first_class_target
        || color_blend_passthrough
        || !material_graph_supported
        || quad.texture_region.is_some()
        || quad.effect_passes.len() != 1
    {
        return false;
    }
    if let Some(mesh) = quad.mesh.as_ref()
        && !native_vulkan_scene_we_image_pass_chain_mesh_is_full_quad(quad, mesh)
    {
        return false;
    }
    let effect = &quad.effect_passes[0];
    if effect.target.is_some()
        || effect.source.is_some()
        || !effect.fbos.is_empty()
        || effect.binds.contains_key(&0)
    {
        return false;
    }
    matches!(
        effect.kind,
        NativeVulkanSceneEffectKind::WaterRipple
            | NativeVulkanSceneEffectKind::WaterWaves
            | NativeVulkanSceneEffectKind::WaterFlow
            | NativeVulkanSceneEffectKind::WaterCaustics
            | NativeVulkanSceneEffectKind::FoliageSway
            | NativeVulkanSceneEffectKind::Scroll
    )
}

fn native_vulkan_scene_we_image_pass_chain_can_fold_color_blend_passthrough(
    quad: &NativeVulkanSceneSampledImageQuad,
) -> bool {
    let Some(effect) = quad.effect_passes.last() else {
        return false;
    };
    if effect.target.is_some()
        || effect.source.is_some()
        || !effect.fbos.is_empty()
        || effect.binds.contains_key(&0)
        || !native_vulkan_scene_effect_pass_has_executable_material_graph(effect)
    {
        return false;
    }
    let effect_blend = effect
        .blending
        .as_deref()
        .and_then(scene_blend_mode_from_material_blending)
        .unwrap_or(SceneBlendMode::Normal);
    effect_blend == SceneBlendMode::Normal
}

fn native_vulkan_scene_we_image_pass_chain_mesh_is_full_quad(
    quad: &NativeVulkanSceneSampledImageQuad,
    mesh: &SceneMesh,
) -> bool {
    if mesh.vertices.len() != 4
        || mesh.indices.len() != 6
        || !quad.width.is_finite()
        || !quad.height.is_finite()
        || quad.width <= 0.0
        || quad.height <= 0.0
    {
        return false;
    }
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    let mut min_u = f64::INFINITY;
    let mut max_u = f64::NEG_INFINITY;
    let mut min_v = f64::INFINITY;
    let mut max_v = f64::NEG_INFINITY;
    for vertex in &mesh.vertices {
        if !vertex.x.is_finite()
            || !vertex.y.is_finite()
            || !vertex.u.is_finite()
            || !vertex.v.is_finite()
        {
            return false;
        }
        min_x = min_x.min(vertex.x);
        max_x = max_x.max(vertex.x);
        min_y = min_y.min(vertex.y);
        max_y = max_y.max(vertex.y);
        min_u = min_u.min(vertex.u);
        max_u = max_u.max(vertex.u);
        min_v = min_v.min(vertex.v);
        max_v = max_v.max(vertex.v);
    }
    let extent = quad.width.max(quad.height).max(1.0);
    let eps = (extent * 1.0e-4).max(1.0e-4);
    (min_x + quad.width * 0.5).abs() <= eps
        && (max_x - quad.width * 0.5).abs() <= eps
        && (min_y + quad.height * 0.5).abs() <= eps
        && (max_y - quad.height * 0.5).abs() <= eps
        && min_u.abs() <= eps
        && (max_u - 1.0).abs() <= eps
        && min_v.abs() <= eps
        && (max_v - 1.0).abs() <= eps
}

fn native_vulkan_scene_effect_pass_has_executable_material_graph(
    pass: &NativeVulkanSceneEffectRecord,
) -> bool {
    match pass.kind {
        NativeVulkanSceneEffectKind::OpacityMask
        | NativeVulkanSceneEffectKind::Scroll
        | NativeVulkanSceneEffectKind::WaterRipple
        | NativeVulkanSceneEffectKind::WaterFlow
        | NativeVulkanSceneEffectKind::WaterWaves
        | NativeVulkanSceneEffectKind::WaterCaustics => {
            !native_vulkan_scene_effect_combo_enabled(pass, "PERSPECTIVE")
        }
        NativeVulkanSceneEffectKind::FoliageSway => {
            native_vulkan_scene_effect_combo_value(pass, "MODE").unwrap_or(0) == 0
        }
        NativeVulkanSceneEffectKind::AutoSway => {
            native_vulkan_scene_effect_combo_value(pass, "AA_VERSION")
                .is_none_or(|value| value == 2)
                && !native_vulkan_scene_effect_combo_enabled(pass, "NOISE")
                && !native_vulkan_scene_effect_combo_enabled(pass, "EXPONENT")
                && !native_vulkan_scene_effect_combo_enabled(pass, "AUTO_TIMEOFFSET_INTERPOLATION")
        }
        _ => false,
    }
}

fn native_vulkan_scene_effect_combo_value(
    pass: &NativeVulkanSceneEffectRecord,
    key: &str,
) -> Option<i64> {
    pass.combo_values
        .iter()
        .find_map(|(candidate, value)| candidate.eq_ignore_ascii_case(key).then_some(*value))
}

fn native_vulkan_scene_effect_combo_enabled(
    pass: &NativeVulkanSceneEffectRecord,
    key: &str,
) -> bool {
    native_vulkan_scene_effect_combo_value(pass, key)
        .map(|value| value != 0)
        .unwrap_or_else(|| {
            pass.combo_keys
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(key))
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
        let bounds = native_vulkan_scene_we_image_graph_target_bounds(quad, chain, scale);
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
            local_left: bounds.left,
            local_top: bounds.top,
            width: native_vulkan_scene_we_image_graph_target_extent(bounds.width),
            height: native_vulkan_scene_we_image_graph_target_extent(bounds.height),
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
            let bounds =
                native_vulkan_scene_we_image_graph_target_bounds(quad, chain, Some(fbo.scale));
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
                local_left: bounds.left,
                local_top: bounds.top,
                width: native_vulkan_scene_we_image_graph_target_extent(bounds.width),
                height: native_vulkan_scene_we_image_graph_target_extent(bounds.height),
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

fn native_vulkan_scene_we_image_graph_target_bounds(
    quad: &NativeVulkanSceneSampledImageQuad,
    chain: &NativeVulkanSceneWeImagePassChain,
    scale: Option<f64>,
) -> NativeVulkanSceneWeImageGraphTargetBounds {
    let mut bounds =
        if native_vulkan_scene_we_image_pass_chain_uses_layer_uv_domain_puppet_targets(quad, chain)
        {
            native_vulkan_scene_we_image_graph_layer_uv_domain_target_bounds(quad)
        } else {
            quad.mesh
                .as_deref()
                .and_then(|mesh| native_vulkan_scene_we_image_graph_mesh_target_bounds(quad, mesh))
                .unwrap_or_else(|| native_vulkan_scene_we_image_graph_nominal_target_bounds(quad))
        };
    if let Some(scale) = scale.filter(|scale| scale.is_finite() && *scale > 0.0) {
        bounds.width *= scale;
        bounds.height *= scale;
    }
    bounds
}

pub(in crate::renderer::native_vulkan::scene) fn native_vulkan_scene_we_image_pass_chain_uses_layer_uv_domain_puppet_targets(
    quad: &NativeVulkanSceneSampledImageQuad,
    chain: &NativeVulkanSceneWeImagePassChain,
) -> bool {
    quad.mesh.is_some()
        && chain
            .passes
            .iter()
            .filter(|pass| pass.role == NativeVulkanSceneWeImagePassRole::EffectMaterial)
            .all(|pass| pass.effect_kind == Some(NativeVulkanSceneEffectKind::WaterWaves))
        && chain
            .passes
            .iter()
            .any(|pass| pass.effect_kind == Some(NativeVulkanSceneEffectKind::WaterWaves))
}

fn native_vulkan_scene_we_image_graph_layer_uv_domain_target_bounds(
    quad: &NativeVulkanSceneSampledImageQuad,
) -> NativeVulkanSceneWeImageGraphTargetBounds {
    let Some(mesh) = quad.mesh.as_deref() else {
        return native_vulkan_scene_we_image_graph_nominal_target_bounds(quad);
    };
    if !quad.width.is_finite()
        || !quad.height.is_finite()
        || quad.width <= f64::EPSILON
        || quad.height <= f64::EPSILON
    {
        return native_vulkan_scene_we_image_graph_nominal_target_bounds(quad);
    }

    let mut min_u = 0.0f64;
    let mut max_u = 1.0f64;
    let mut min_v = 0.0f64;
    let mut max_v = 1.0f64;
    let mut saw_mesh_uv = false;
    for vertex in &mesh.vertices {
        if vertex.u.is_finite() && vertex.v.is_finite() {
            min_u = min_u.min(vertex.u);
            max_u = max_u.max(vertex.u);
            min_v = min_v.min(vertex.v);
            max_v = max_v.max(vertex.v);
            saw_mesh_uv = true;
        }
    }
    if !saw_mesh_uv || max_u <= min_u || max_v <= min_v {
        return native_vulkan_scene_we_image_graph_nominal_target_bounds(quad);
    }
    NativeVulkanSceneWeImageGraphTargetBounds {
        left: min_u * quad.width,
        top: (1.0 - max_v) * quad.height,
        width: (max_u - min_u) * quad.width,
        height: (max_v - min_v) * quad.height,
    }
}

fn native_vulkan_scene_we_image_graph_nominal_target_bounds(
    quad: &NativeVulkanSceneSampledImageQuad,
) -> NativeVulkanSceneWeImageGraphTargetBounds {
    NativeVulkanSceneWeImageGraphTargetBounds {
        left: 0.0,
        top: 0.0,
        width: quad.width.max(1.0),
        height: quad.height.max(1.0),
    }
}

fn native_vulkan_scene_we_image_graph_mesh_target_bounds(
    quad: &NativeVulkanSceneSampledImageQuad,
    mesh: &SceneMesh,
) -> Option<NativeVulkanSceneWeImageGraphTargetBounds> {
    if mesh.vertices.is_empty()
        || !quad.width.is_finite()
        || !quad.height.is_finite()
        || quad.width <= 0.0
        || quad.height <= 0.0
    {
        return None;
    }
    let mut left = f64::INFINITY;
    let mut top = f64::INFINITY;
    let mut right = f64::NEG_INFINITY;
    let mut bottom = f64::NEG_INFINITY;
    for vertex in &mesh.vertices {
        if !vertex.x.is_finite() || !vertex.y.is_finite() {
            return None;
        }
        let x = vertex.x + quad.width * 0.5;
        let y = vertex.y + quad.height * 0.5;
        left = left.min(x);
        top = top.min(y);
        right = right.max(x);
        bottom = bottom.max(y);
    }
    if !left.is_finite() || !top.is_finite() || !right.is_finite() || !bottom.is_finite() {
        return None;
    }
    let left_overhang = (-left).max(0.0);
    let right_overhang = (right - quad.width).max(0.0);
    let top_overhang = (-top).max(0.0);
    let bottom_overhang = (bottom - quad.height).max(0.0);
    let x_margin = if left_overhang > f64::EPSILON || right_overhang > f64::EPSILON {
        left_overhang
            .max(right_overhang)
            .max(quad.width * 0.25)
            .ceil()
    } else {
        0.0
    };
    let y_margin = if top_overhang > f64::EPSILON || bottom_overhang > f64::EPSILON {
        top_overhang
            .max(bottom_overhang)
            .max(quad.height * 0.10)
            .ceil()
    } else {
        0.0
    };
    Some(NativeVulkanSceneWeImageGraphTargetBounds {
        left: -x_margin,
        top: -y_margin,
        width: (quad.width + x_margin * 2.0).max(1.0),
        height: (quad.height + y_margin * 2.0).max(1.0),
    })
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::Arc;

    use crate::core::scene::{SceneMesh, SceneMeshVertex, SceneNativeEffectMotion};
    use crate::core::{FitMode, SceneBlendMode, SceneTransform};
    use crate::renderer::SceneRenderAlphaTextureMode;

    use super::*;
    use crate::renderer::native_vulkan::scene::draw_pass::{
        NativeVulkanSceneEffectEvaluationBoundary, NativeVulkanSceneEffectKind,
        NativeVulkanSceneEffectRecord, NativeVulkanSceneMaterialFlag,
        NativeVulkanSceneMaterialKind, NativeVulkanSceneMaterialPass,
        NativeVulkanSceneSampledImageEffectPass, NativeVulkanSceneTextureSlot,
    };

    fn iris_effect_record() -> NativeVulkanSceneEffectRecord {
        NativeVulkanSceneEffectRecord {
            kind: NativeVulkanSceneEffectKind::Iris,
            evaluation_boundary: NativeVulkanSceneEffectEvaluationBoundary::FirstClassTarget,
            effect_file: "effects/iris/effect.json".to_owned(),
            runtime: Some("native-iris-mask".to_owned()),
            pass_index: 0,
            command: None,
            source: None,
            target: None,
            binds: BTreeMap::new(),
            fbos: Vec::new(),
            shader: Some("effects/iris".to_owned()),
            blending: Some("normal".to_owned()),
            texture_slots: vec![NativeVulkanSceneTextureSlot {
                slot: 1,
                source: PathBuf::from("/tmp/iris-mask.gtex"),
                width: Some(331),
                height: Some(115),
            }],
            effect_uv_transform: None,
            parameter_keys: Vec::new(),
            constant_shader_values: BTreeMap::new(),
            combo_keys: Vec::new(),
            combo_values: BTreeMap::new(),
            depth_test: NativeVulkanSceneMaterialFlag::Disabled,
            depth_write: NativeVulkanSceneMaterialFlag::Disabled,
            cull_mode: NativeVulkanSceneCullMode::None,
        }
    }

    fn mesh() -> Arc<SceneMesh> {
        Arc::new(SceneMesh {
            vertices: vec![
                SceneMeshVertex {
                    x: 0.0,
                    y: 0.0,
                    u: 0.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: 1.0,
                    y: 0.0,
                    u: 1.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: 0.0,
                    y: 1.0,
                    u: 0.0,
                    v: 1.0,
                    opacity: 1.0,
                },
            ],
            indices: vec![0, 1, 2],
            skin: None,
            puppet_clips: Vec::new(),
        })
    }

    fn full_quad_mesh(width: f64, height: f64) -> Arc<SceneMesh> {
        Arc::new(SceneMesh {
            vertices: vec![
                SceneMeshVertex {
                    x: -width * 0.5,
                    y: -height * 0.5,
                    u: 0.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: width * 0.5,
                    y: -height * 0.5,
                    u: 1.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: -width * 0.5,
                    y: height * 0.5,
                    u: 0.0,
                    v: 1.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: width * 0.5,
                    y: height * 0.5,
                    u: 1.0,
                    v: 1.0,
                    opacity: 1.0,
                },
            ],
            indices: vec![0, 1, 2, 2, 1, 3],
            skin: None,
            puppet_clips: Vec::new(),
        })
    }

    fn sampled_image_quad(mesh: Option<Arc<SceneMesh>>) -> NativeVulkanSceneSampledImageQuad {
        let effect_passes = vec![iris_effect_record()];
        NativeVulkanSceneSampledImageQuad {
            layer_index: 7,
            layer_id: "eye".to_owned(),
            source: PathBuf::from("/tmp/eye.gtex"),
            texture_slots: vec![NativeVulkanSceneTextureSlot {
                slot: 0,
                source: PathBuf::from("/tmp/eye.gtex"),
                width: Some(663),
                height: Some(230),
            }],
            image_effect_pass_count: effect_passes.len(),
            effect_target_pass: Some(NativeVulkanSceneSampledImageEffectPass {
                texture_slots: vec![NativeVulkanSceneTextureSlot {
                    slot: 1,
                    source: PathBuf::from("/tmp/iris-mask.gtex"),
                    width: Some(331),
                    height: Some(115),
                }],
                alpha_texture_slot: Some(1),
                alpha_texture_mode: SceneRenderAlphaTextureMode::Iris,
                effect_uv_transform: None,
            }),
            material_pass: NativeVulkanSceneMaterialPass {
                kind: NativeVulkanSceneMaterialKind::SampledImage,
                shader: Some("genericimage4".to_owned()),
                blending: Some("translucent".to_owned()),
                render_state: native_vulkan_scene_render_state(
                    SceneBlendMode::Alpha,
                    NativeVulkanSceneMaterialFlag::Disabled,
                    NativeVulkanSceneMaterialFlag::Disabled,
                    NativeVulkanSceneCullMode::Back,
                ),
                alpha_texture_slot: None,
                alpha_texture_mode: SceneRenderAlphaTextureMode::Multiply,
                texture_slot_count: 1,
                effect_kinds: Vec::new(),
                constant_shader_values: BTreeMap::new(),
                system_shader_uniforms: Vec::new(),
                combo_keys: Vec::new(),
                combo_values: BTreeMap::new(),
            },
            base_blend_mode: SceneBlendMode::Alpha,
            effect_passes,
            composite_key: None,
            fit: FitMode::Cover,
            opacity: 1.0,
            tint: [1.0, 1.0, 1.0, 1.0],
            width: 663.0,
            height: 230.0,
            mesh,
            effect_uv_space: None,
            effect_motion: SceneNativeEffectMotion::default(),
            texture_region: None,
            transform: SceneTransform::default(),
        }
    }

    fn layer_bounds_mesh(
        layer_width: f64,
        layer_height: f64,
        left: f64,
        top: f64,
        right: f64,
        bottom: f64,
    ) -> Arc<SceneMesh> {
        Arc::new(SceneMesh {
            vertices: vec![
                SceneMeshVertex {
                    x: left - layer_width * 0.5,
                    y: top - layer_height * 0.5,
                    u: 0.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: right - layer_width * 0.5,
                    y: top - layer_height * 0.5,
                    u: 1.0,
                    v: 0.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: left - layer_width * 0.5,
                    y: bottom - layer_height * 0.5,
                    u: 0.0,
                    v: 1.0,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: right - layer_width * 0.5,
                    y: bottom - layer_height * 0.5,
                    u: 1.0,
                    v: 1.0,
                    opacity: 1.0,
                },
            ],
            indices: vec![0, 1, 2, 2, 1, 3],
            skin: None,
            puppet_clips: Vec::new(),
        })
    }

    #[test]
    fn animated_overhang_target_bounds_do_not_track_minor_puppet_bbox_changes() {
        let mut first = sampled_image_quad(Some(layer_bounds_mesh(
            100.0, 100.0, -20.0, 0.0, 120.0, 100.0,
        )));
        first.width = 100.0;
        first.height = 100.0;
        let mut later = sampled_image_quad(Some(layer_bounds_mesh(
            100.0, 100.0, -24.0, 0.0, 123.0, 100.0,
        )));
        later.width = 100.0;
        later.height = 100.0;

        let first_chain = native_vulkan_scene_we_image_pass_chain(&first).expect("first chain");
        let later_chain = native_vulkan_scene_we_image_pass_chain(&later).expect("later chain");
        let first_bounds =
            native_vulkan_scene_we_image_graph_target_bounds(&first, &first_chain, None);
        let later_bounds =
            native_vulkan_scene_we_image_graph_target_bounds(&later, &later_chain, None);

        assert_eq!(first_bounds.left, -25.0);
        assert_eq!(first_bounds.width, 150.0);
        assert_eq!(first_bounds, later_bounds);
    }

    #[test]
    fn waterwaves_puppet_targets_follow_layer_uv_domain() {
        let mut effect = iris_effect_record();
        effect.kind = NativeVulkanSceneEffectKind::WaterWaves;
        effect.evaluation_boundary = NativeVulkanSceneEffectEvaluationBoundary::MaterialPass;
        effect.effect_file = "effects/waterwaves/effect.json".to_owned();
        effect.runtime = Some("native-waterwaves".to_owned());
        effect.shader = Some("effects/waterwaves".to_owned());
        let mut quad = sampled_image_quad(Some(layer_bounds_mesh(
            100.0, 100.0, -40.0, 0.0, 160.0, 100.0,
        )));
        quad.width = 100.0;
        quad.height = 100.0;
        quad.effect_target_pass = None;
        quad.effect_passes = vec![effect];
        quad.image_effect_pass_count = 1;

        let chain = native_vulkan_scene_we_image_pass_chain(&quad).expect("waterwaves chain");
        let bounds = native_vulkan_scene_we_image_graph_target_bounds(&quad, &chain, None);

        assert!(
            native_vulkan_scene_we_image_pass_chain_uses_layer_uv_domain_puppet_targets(
                &quad, &chain
            )
        );
        assert_eq!(bounds.left, 0.0);
        assert_eq!(bounds.top, 0.0);
        assert_eq!(bounds.width, 100.0);
        assert_eq!(bounds.height, 100.0);
    }

    #[test]
    fn waterwaves_puppet_targets_include_uv_overhang_without_position_bbox() {
        let mut effect = iris_effect_record();
        effect.kind = NativeVulkanSceneEffectKind::WaterWaves;
        effect.evaluation_boundary = NativeVulkanSceneEffectEvaluationBoundary::MaterialPass;
        effect.effect_file = "effects/waterwaves/effect.json".to_owned();
        effect.runtime = Some("native-waterwaves".to_owned());
        effect.shader = Some("effects/waterwaves".to_owned());
        let mut quad = sampled_image_quad(Some(Arc::new(SceneMesh {
            vertices: vec![
                SceneMeshVertex {
                    x: -80.0,
                    y: -40.0,
                    u: -0.25,
                    v: -0.10,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: 180.0,
                    y: -40.0,
                    u: 1.25,
                    v: -0.10,
                    opacity: 1.0,
                },
                SceneMeshVertex {
                    x: -80.0,
                    y: 140.0,
                    u: -0.25,
                    v: 1.10,
                    opacity: 1.0,
                },
            ],
            indices: vec![0, 1, 2],
            skin: None,
            puppet_clips: Vec::new(),
        })));
        quad.width = 100.0;
        quad.height = 100.0;
        quad.effect_target_pass = None;
        quad.effect_passes = vec![effect];
        quad.image_effect_pass_count = 1;

        let chain = native_vulkan_scene_we_image_pass_chain(&quad).expect("waterwaves chain");
        let bounds = native_vulkan_scene_we_image_graph_target_bounds(&quad, &chain, None);

        assert_eq!(bounds.left, -25.0);
        assert!((bounds.top - -10.0).abs() < 1.0e-6);
        assert_eq!(bounds.width, 150.0);
        assert!((bounds.height - 120.0).abs() < 1.0e-6);
    }

    #[test]
    fn water_ripple_and_flow_material_graph_executes_first_class() {
        let mut ripple = iris_effect_record();
        ripple.kind = NativeVulkanSceneEffectKind::WaterRipple;
        ripple.evaluation_boundary = NativeVulkanSceneEffectEvaluationBoundary::MaterialPass;
        ripple.effect_file = "effects/waterripple/effect.json".to_owned();
        ripple.runtime = Some("native-effect-motion".to_owned());
        ripple.shader = Some("effects/waterripple".to_owned());
        ripple.texture_slots = vec![NativeVulkanSceneTextureSlot {
            slot: 2,
            source: PathBuf::from("/tmp/waterripplenormal.gtex"),
            width: Some(512),
            height: Some(512),
        }];

        let mut flow = iris_effect_record();
        flow.kind = NativeVulkanSceneEffectKind::WaterFlow;
        flow.evaluation_boundary = NativeVulkanSceneEffectEvaluationBoundary::MaterialPass;
        flow.effect_file = "effects/waterflow/effect.json".to_owned();
        flow.runtime = Some("wallpaper-engine-effect".to_owned());
        flow.pass_index = 1;
        flow.shader = Some("effects/waterflow".to_owned());
        flow.texture_slots = vec![
            NativeVulkanSceneTextureSlot {
                slot: 1,
                source: PathBuf::from("/tmp/waterflow-mask.gtex"),
                width: Some(512),
                height: Some(256),
            },
            NativeVulkanSceneTextureSlot {
                slot: 2,
                source: PathBuf::from("/tmp/waterflowphase.gtex"),
                width: Some(64),
                height: Some(64),
            },
        ];

        let mut quad = sampled_image_quad(None);
        quad.effect_target_pass = None;
        quad.effect_passes = vec![ripple, flow];
        quad.image_effect_pass_count = quad.effect_passes.len();

        let chain =
            native_vulkan_scene_we_image_pass_chain(&quad).expect("water material graph chain");
        let plan = native_vulkan_scene_we_image_graph_plan(&[quad]);

        assert_eq!(
            chain.execution,
            NativeVulkanSceneWeImagePassExecution::FirstClassTarget
        );
        assert!(!chain.raw_direct_composite_allowed);
        assert_eq!(plan.first_class_target_chain_count, 1);
        assert_eq!(plan.suppressed_chain_count, 0);
        assert_eq!(
            plan.effect_kind_counts.get("water-ripple").copied(),
            Some(1)
        );
        assert_eq!(plan.effect_kind_counts.get("water-flow").copied(), Some(1));
    }

    #[test]
    fn foliage_sway_uv_material_graph_executes_with_water_ripple() {
        let mut foliage = iris_effect_record();
        foliage.kind = NativeVulkanSceneEffectKind::FoliageSway;
        foliage.evaluation_boundary = NativeVulkanSceneEffectEvaluationBoundary::MaterialPass;
        foliage.effect_file = "effects/workshop/2790231929/foliagesway/effect.json".to_owned();
        foliage.runtime = Some("wallpaper-engine-effect".to_owned());
        foliage.shader = Some("workshop/2790231929/effects/foliagesway".to_owned());
        foliage.texture_slots = vec![NativeVulkanSceneTextureSlot {
            slot: 2,
            source: PathBuf::from("/tmp/noise.gtex"),
            width: Some(256),
            height: Some(256),
        }];

        let mut ripple = iris_effect_record();
        ripple.kind = NativeVulkanSceneEffectKind::WaterRipple;
        ripple.evaluation_boundary = NativeVulkanSceneEffectEvaluationBoundary::MaterialPass;
        ripple.effect_file = "effects/workshop/2790231929/waterripple/effect.json".to_owned();
        ripple.runtime = Some("native-effect-motion".to_owned());
        ripple.pass_index = 1;
        ripple.shader = Some("workshop/2790231929/effects/waterripple".to_owned());
        ripple.texture_slots = vec![NativeVulkanSceneTextureSlot {
            slot: 2,
            source: PathBuf::from("/tmp/waterripplenormal.gtex"),
            width: Some(512),
            height: Some(512),
        }];

        let mut quad = sampled_image_quad(None);
        quad.effect_target_pass = None;
        quad.effect_passes = vec![foliage, ripple];
        quad.image_effect_pass_count = quad.effect_passes.len();

        let chain =
            native_vulkan_scene_we_image_pass_chain(&quad).expect("foliage water graph chain");
        let plan = native_vulkan_scene_we_image_graph_plan(&[quad]);

        assert_eq!(
            chain.execution,
            NativeVulkanSceneWeImagePassExecution::FirstClassTarget
        );
        assert_eq!(plan.first_class_target_chain_count, 1);
        assert_eq!(plan.suppressed_chain_count, 0);
        assert_eq!(
            plan.effect_kind_counts.get("foliage-sway").copied(),
            Some(1)
        );
        assert_eq!(
            plan.effect_kind_counts.get("water-ripple").copied(),
            Some(1)
        );
    }

    #[test]
    fn foliage_sway_mode_combo_uses_value_not_key_presence() {
        let mut foliage = iris_effect_record();
        foliage.kind = NativeVulkanSceneEffectKind::FoliageSway;
        foliage.evaluation_boundary = NativeVulkanSceneEffectEvaluationBoundary::MaterialPass;
        foliage.effect_file = "effects/workshop/2790231929/foliagesway/effect.json".to_owned();
        foliage.combo_keys = vec!["MODE".to_owned()];
        foliage.combo_values = BTreeMap::from([("MODE".to_owned(), 0)]);

        assert!(native_vulkan_scene_effect_pass_has_executable_material_graph(&foliage));

        foliage.combo_values.insert("MODE".to_owned(), 1);
        assert!(!native_vulkan_scene_effect_pass_has_executable_material_graph(&foliage));
    }

    #[test]
    fn single_terminal_waterwaves_executes_directly_without_local_target() {
        let mut waves = iris_effect_record();
        waves.kind = NativeVulkanSceneEffectKind::WaterWaves;
        waves.evaluation_boundary = NativeVulkanSceneEffectEvaluationBoundary::MaterialPass;
        waves.effect_file = "effects/waterwaves/effect.json".to_owned();
        waves.runtime = Some("native-effect-motion".to_owned());
        waves.shader = Some("effects/waterwaves".to_owned());
        waves.texture_slots = vec![NativeVulkanSceneTextureSlot {
            slot: 1,
            source: PathBuf::from("/tmp/waterwaves-mask.gtex"),
            width: Some(512),
            height: Some(256),
        }];

        let mut quad = sampled_image_quad(Some(full_quad_mesh(663.0, 230.0)));
        quad.effect_target_pass = None;
        quad.effect_passes = vec![waves];
        quad.image_effect_pass_count = quad.effect_passes.len();

        let chain = native_vulkan_scene_we_image_pass_chain(&quad)
            .expect("direct terminal waterwaves graph chain");
        let plan = native_vulkan_scene_we_image_graph_plan(&[quad]);

        assert_eq!(
            chain.execution,
            NativeVulkanSceneWeImagePassExecution::FirstClassTarget
        );
        assert!(!chain.local_target_required);
        assert!(!chain.first_pass_blend_moved_to_final);
        assert_eq!(chain.passes.len(), 1);
        assert_eq!(
            chain.passes[0].role,
            NativeVulkanSceneWeImagePassRole::EffectMaterial
        );
        assert_eq!(
            chain.passes[0].input,
            NativeVulkanSceneWeImagePassEndpoint::SourceTexture
        );
        assert_eq!(
            chain.passes[0].target,
            NativeVulkanSceneWeImagePassEndpoint::Scene
        );
        assert!(chain.passes[0].final_scene_pass);
        assert_eq!(plan.target_count, 0);
        assert_eq!(plan.step_count, 1);
        assert_eq!(
            plan.steps[0].texture_bindings[0].source,
            NativeVulkanSceneWeImageGraphTextureBindingSource::SourceTexture
        );
        assert_eq!(plan.steps[0].texture_bindings[0].slot, 0);
        assert_eq!(plan.steps[0].texture_bindings[1].slot, 1);
    }

    #[test]
    fn single_terminal_watercaustics_executes_directly_without_local_target() {
        let mut caustics = iris_effect_record();
        caustics.kind = NativeVulkanSceneEffectKind::WaterCaustics;
        caustics.evaluation_boundary = NativeVulkanSceneEffectEvaluationBoundary::MaterialPass;
        caustics.effect_file = "effects/watercaustics/effect.json".to_owned();
        caustics.runtime = Some("wallpaper-engine-effect".to_owned());
        caustics.shader = Some("effects/caustics".to_owned());
        caustics.combo_values = BTreeMap::from([("BLENDMODE".to_owned(), 6)]);
        caustics.texture_slots = vec![
            NativeVulkanSceneTextureSlot {
                slot: 2,
                source: PathBuf::from("/tmp/pattern/voronoi_local.gtex"),
                width: Some(256),
                height: Some(256),
            },
            NativeVulkanSceneTextureSlot {
                slot: 3,
                source: PathBuf::from("/tmp/util/uniform_256.gtex"),
                width: Some(256),
                height: Some(256),
            },
            NativeVulkanSceneTextureSlot {
                slot: 4,
                source: PathBuf::from("/tmp/util/perlin_256.gtex"),
                width: Some(256),
                height: Some(256),
            },
            NativeVulkanSceneTextureSlot {
                slot: 5,
                source: PathBuf::from("/tmp/pattern/voronoi.gtex"),
                width: Some(256),
                height: Some(256),
            },
        ];

        let mut quad = sampled_image_quad(Some(full_quad_mesh(663.0, 230.0)));
        quad.effect_target_pass = None;
        quad.effect_passes = vec![caustics];
        quad.image_effect_pass_count = quad.effect_passes.len();

        let chain = native_vulkan_scene_we_image_pass_chain(&quad)
            .expect("direct terminal watercaustics graph chain");
        let plan = native_vulkan_scene_we_image_graph_plan(&[quad]);

        assert_eq!(
            chain.execution,
            NativeVulkanSceneWeImagePassExecution::FirstClassTarget
        );
        assert!(!chain.local_target_required);
        assert_eq!(chain.passes.len(), 1);
        assert_eq!(
            chain.passes[0].effect_kind,
            Some(NativeVulkanSceneEffectKind::WaterCaustics)
        );
        assert_eq!(
            chain.passes[0].target,
            NativeVulkanSceneWeImagePassEndpoint::Scene
        );
        assert_eq!(plan.target_count, 0);
        assert_eq!(plan.step_count, 1);
        assert_eq!(
            plan.effect_kind_counts.get("water-caustics").copied(),
            Some(1)
        );
        assert_eq!(
            plan.steps[0].texture_bindings[0].source,
            NativeVulkanSceneWeImageGraphTextureBindingSource::SourceTexture
        );
        assert_eq!(plan.steps[0].texture_bindings[0].slot, 0);
        assert_eq!(plan.steps[0].texture_bindings.len(), 5);
    }

    #[test]
    fn single_terminal_scroll_executes_directly_without_raw_fallback() {
        let mut scroll = iris_effect_record();
        scroll.kind = NativeVulkanSceneEffectKind::Scroll;
        scroll.evaluation_boundary = NativeVulkanSceneEffectEvaluationBoundary::MaterialPass;
        scroll.effect_file = "effects/scroll/effect.json".to_owned();
        scroll.runtime = None;
        scroll.shader = Some("effects/scroll".to_owned());
        scroll.texture_slots.clear();
        scroll.constant_shader_values = BTreeMap::from([
            ("speedx".to_owned(), serde_json::json!(0.1)),
            ("speedy".to_owned(), serde_json::json!(0.0)),
            ("repeat".to_owned(), serde_json::json!("1 1")),
        ]);

        let mut quad = sampled_image_quad(Some(full_quad_mesh(2457.0, 616.0)));
        quad.effect_target_pass = None;
        quad.effect_passes = vec![scroll];
        quad.image_effect_pass_count = quad.effect_passes.len();
        quad.width = 2457.0;
        quad.height = 616.0;

        let chain =
            native_vulkan_scene_we_image_pass_chain(&quad).expect("direct terminal scroll chain");
        let plan = native_vulkan_scene_we_image_graph_plan(&[quad]);

        assert_eq!(
            chain.execution,
            NativeVulkanSceneWeImagePassExecution::FirstClassTarget
        );
        assert!(!chain.local_target_required);
        assert_eq!(chain.passes.len(), 1);
        assert_eq!(
            chain.passes[0].effect_kind,
            Some(NativeVulkanSceneEffectKind::Scroll)
        );
        assert_eq!(plan.first_class_target_chain_count, 1);
        assert_eq!(plan.temporary_raw_fallback_chain_count, 0);
        assert_eq!(plan.steps[0].step_index, 0);
    }

    #[test]
    fn clipping_mask_chain_without_runtime_texture_is_suppressed_not_raw_static() {
        let mut scroll = iris_effect_record();
        scroll.kind = NativeVulkanSceneEffectKind::Scroll;
        scroll.evaluation_boundary = NativeVulkanSceneEffectEvaluationBoundary::MaterialPass;
        scroll.effect_file = "effects/scroll/effect.json".to_owned();
        scroll.runtime = None;
        scroll.shader = Some("effects/scroll".to_owned());
        scroll.texture_slots.clear();

        let mut clipping = iris_effect_record();
        clipping.kind = NativeVulkanSceneEffectKind::ClippingMask;
        clipping.evaluation_boundary = NativeVulkanSceneEffectEvaluationBoundary::MaterialPass;
        clipping.effect_file = "effects/workshop/2800594362/clipping_mask/effect.json".to_owned();
        clipping.runtime = None;
        clipping.pass_index = 1;
        clipping.shader = Some("workshop/2800594362/effects/clipping_mask".to_owned());
        clipping.texture_slots.clear();
        clipping.combo_values = BTreeMap::from([("REPEAT".to_owned(), 1)]);

        let mut quad = sampled_image_quad(None);
        quad.effect_target_pass = None;
        quad.effect_passes = vec![scroll, clipping];
        quad.image_effect_pass_count = quad.effect_passes.len();
        quad.width = 2457.0;
        quad.height = 616.0;

        let chain =
            native_vulkan_scene_we_image_pass_chain(&quad).expect("scroll plus clipping chain");
        let plan = native_vulkan_scene_we_image_graph_plan(&[quad]);

        assert_eq!(
            chain.execution,
            NativeVulkanSceneWeImagePassExecution::SuppressedUntilGraphExecutor
        );
        assert!(!chain.raw_direct_composite_allowed);
        assert_eq!(
            chain.unsupported_reason,
            Some("we-effect-graph-material-pass-not-executed")
        );
        assert_eq!(plan.suppressed_chain_count, 1);
        assert_eq!(plan.temporary_raw_fallback_chain_count, 0);
    }

    #[test]
    fn auto_sway_material_graph_executes_before_waterwaves() {
        let mut auto_sway = iris_effect_record();
        auto_sway.kind = NativeVulkanSceneEffectKind::AutoSway;
        auto_sway.evaluation_boundary = NativeVulkanSceneEffectEvaluationBoundary::MaterialPass;
        auto_sway.effect_file = "effects/workshop/3392386920/auto_sway/effect.json".to_owned();
        auto_sway.runtime = Some("native-effect-motion".to_owned());
        auto_sway.shader = Some("workshop/3392386920/effects/auto_sway".to_owned());
        auto_sway.combo_keys = vec![
            "DEBUG".to_owned(),
            "DEBUG_NO_ALPHA".to_owned(),
            "NODE_COUNT".to_owned(),
        ];

        let mut waves = iris_effect_record();
        waves.kind = NativeVulkanSceneEffectKind::WaterWaves;
        waves.evaluation_boundary = NativeVulkanSceneEffectEvaluationBoundary::MaterialPass;
        waves.effect_file = "effects/waterwaves/effect.json".to_owned();
        waves.runtime = Some("native-effect-motion".to_owned());
        waves.pass_index = 1;
        waves.shader = Some("effects/waterwaves".to_owned());

        let mut quad = sampled_image_quad(None);
        quad.effect_target_pass = None;
        quad.effect_passes = vec![auto_sway, waves];
        quad.image_effect_pass_count = quad.effect_passes.len();

        let chain = native_vulkan_scene_we_image_pass_chain(&quad)
            .expect("auto_sway waterwaves graph chain");
        let plan = native_vulkan_scene_we_image_graph_plan(&[quad]);

        assert_eq!(
            chain.execution,
            NativeVulkanSceneWeImagePassExecution::FirstClassTarget
        );
        assert_eq!(plan.first_class_target_chain_count, 1);
        assert_eq!(plan.suppressed_chain_count, 0);
        assert_eq!(plan.effect_kind_counts.get("auto-sway").copied(), Some(1));
        assert_eq!(plan.effect_kind_counts.get("water-waves").copied(), Some(1));
    }

    #[test]
    fn puppet_base_pass_keeps_cwe_translucent_blend_and_final_uses_scene_blend() {
        let chain = native_vulkan_scene_we_image_pass_chain(&sampled_image_quad(Some(mesh())))
            .expect("WE graph chain");

        assert!(chain.first_pass_blend_moved_to_final);
        assert_eq!(chain.final_scene_blend_mode, SceneBlendMode::Alpha);
        assert_eq!(
            chain.passes[0].role,
            NativeVulkanSceneWeImagePassRole::BaseMaterial
        );
        assert_eq!(chain.passes[0].scene_blend_mode, SceneBlendMode::Alpha);
        assert_eq!(
            chain.passes[0].render_state.blend.mode,
            SceneBlendMode::Alpha
        );
        assert_eq!(chain.passes[1].scene_blend_mode, SceneBlendMode::Alpha);
        assert_eq!(
            chain.passes[1].render_state.blend.mode,
            SceneBlendMode::Alpha
        );
    }

    #[test]
    fn non_puppet_base_pass_stays_normal_after_final_blend_move() {
        let chain =
            native_vulkan_scene_we_image_pass_chain(&sampled_image_quad(None)).expect("WE graph");

        assert!(chain.first_pass_blend_moved_to_final);
        assert_eq!(
            chain.passes[0].role,
            NativeVulkanSceneWeImagePassRole::BaseMaterial
        );
        assert_eq!(chain.passes[0].scene_blend_mode, SceneBlendMode::Normal);
        assert_eq!(
            chain.passes[0].render_state.blend.mode,
            SceneBlendMode::Normal
        );
        assert_eq!(chain.final_scene_blend_mode, SceneBlendMode::Alpha);
        assert_eq!(chain.passes[1].scene_blend_mode, SceneBlendMode::Alpha);
    }
}
