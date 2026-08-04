//! Dynamic semantic-frame planning and host-visible GPU buffer updates.

use std::time::Instant;

use crate::engine::scene::semantic_world::ResolvedSemanticFrame;
use crate::engine::scene::{
    SceneMaterialHandle, SceneObjectHandle, SceneRenderingDeviceDrawPrimitive,
    SceneRenderingDeviceGraphPlan, SceneRenderingDeviceMeshDraw,
    SceneRenderingDeviceProjectionDomain, SceneStorage,
};

use super::draw_recording::SceneGpuDrawCommand;
use super::scene_color_clear::SceneGpuSceneColorClear;

mod shared;
mod topology;
pub(super) mod video;

#[cfg(test)]
mod visibility_tests;

pub(super) use topology::pack_scene_skinning_palette;
use topology::{resolved_draw_effect_visibility_mask, validate_topology_slice};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct SceneFrameBufferUpdate {
    pub transform_uniform_updated: bool,
    pub material_uniform_updated: bool,
    pub skinning_storage_updated: bool,
    pub scene_owned_uniform_updated: bool,
    pub dynamic_text_instance_updated: bool,
    pub scene_color_attachment_clear: Option<SceneGpuSceneColorClear>,
    pub cpu_timing: SceneFrameCpuTiming,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct SceneFrameCpuTiming {
    pub semantic_resolve_micros: u64,
    pub graph_update_micros: u64,
    pub transform_update_micros: u64,
    pub material_update_micros: u64,
    pub skinning_update_micros: u64,
    pub scene_owned_uniform_update_micros: u64,
    pub draw_policy_update_micros: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct SceneFrameTopology {
    graph: SceneRenderingDeviceGraphPlan,
    draws: Vec<SceneFrameDrawTopology>,
    palettes: Vec<SceneFramePuppetPaletteTopology>,
    bones: Vec<SceneFramePuppetBoneTopology>,
    sampled_target_producers: Vec<SceneFrameSampledTargetProducerTopology>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SceneFrameDrawTopology {
    primitive: SceneRenderingDeviceDrawPrimitive,
    projection_domain: SceneRenderingDeviceProjectionDomain,
    mesh_index: u32,
    resolved_object_index: u32,
    effect_binding_start: u32,
    effect_binding_count: u32,
    effect_visibility_policy: crate::engine::scene::SceneRenderEffectVisibilityPolicy,
    skinning_palette_start: u32,
    skinning_palette_count: u32,
    object: SceneObjectHandle,
    material: SceneMaterialHandle,
    vertex_start: u32,
    vertex_count: u32,
    index_start: u32,
    index_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SceneFramePuppetPaletteTopology {
    object: SceneObjectHandle,
    puppet_index: u32,
    bone_matrix_start: u32,
    bone_matrix_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SceneFramePuppetBoneTopology {
    puppet_index: u32,
    bone_index: u32,
    parent_index: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SceneFrameSampledTargetProducerTopology {
    consumer_pass_node_index: u32,
    producer_pass_node_indices: Vec<u32>,
}

impl SceneFrameTopology {
    pub(super) fn from_graph(graph: &SceneRenderingDeviceGraphPlan) -> Self {
        Self::from_owned_graph(graph.clone())
    }

    pub(super) fn from_owned_graph(graph: SceneRenderingDeviceGraphPlan) -> Self {
        let sampled_target_producers = sampled_target_producer_topology(&graph);
        let draws = graph.mesh_draws.iter().map(draw_topology).collect();
        let palettes = graph
            .puppet_bone_palettes
            .iter()
            .map(|palette| SceneFramePuppetPaletteTopology {
                object: palette.object,
                puppet_index: palette.puppet_index,
                bone_matrix_start: palette.bone_matrix_start,
                bone_matrix_count: palette.bone_matrix_count,
            })
            .collect();
        let bones = graph
            .puppet_bone_matrices
            .iter()
            .map(|bone| SceneFramePuppetBoneTopology {
                puppet_index: bone.puppet_index,
                bone_index: bone.bone_index,
                parent_index: bone.parent_index,
            })
            .collect();
        Self {
            graph,
            draws,
            palettes,
            bones,
            sampled_target_producers,
        }
    }

    pub(super) fn graph(&self) -> &SceneRenderingDeviceGraphPlan {
        &self.graph
    }

    fn update_dynamic_graph(
        &mut self,
        storage: &SceneStorage,
        semantic_frame: &ResolvedSemanticFrame,
        scene_time_seconds: f32,
    ) -> Result<&SceneRenderingDeviceGraphPlan, String> {
        validate_dynamic_counts(&self.graph, semantic_frame, scene_time_seconds)?;
        for draw in &mut self.graph.mesh_draws {
            if draw.object.0 == crate::engine::scene::INVALID_OBJECT_ID {
                continue;
            }
            let object = semantic_frame.object(draw.object).ok_or_else(|| {
                format!(
                    "scene draw object {} disappeared at {scene_time_seconds:.6}s",
                    draw.object.0
                )
            })?;
            draw.resolved_object_index = object.object_index;
            draw.render_world_matrix = rows_from_column_major(object.render_world_matrix);
            draw.clip_transform = draw.projection_domain.clip_transform(
                storage,
                semantic_frame,
                object.render_world_matrix,
            );
            draw.effect_model_view_projection_matrix = draw.clip_transform;
            draw.resolved_color = object.resolved_color;
            draw.resolved_alpha = object.resolved_alpha;
            draw.resolved_effect_visibility_mask =
                resolved_draw_effect_visibility_mask(semantic_frame, draw);
        }
        update_puppet_palettes(&mut self.graph, semantic_frame, scene_time_seconds)?;
        self.graph.resolved_object_count = semantic_frame.objects.len();
        self.graph.resolved_visible_object_count = semantic_frame.visible_object_count;
        self.graph.resolved_attachment_link_count = semantic_frame.attachment_links.len();
        self.graph.resolved_visible_effect_instance_count =
            semantic_frame.visible_effect_instance_count;
        self.graph.resolved_visible_effect_pass_count = semantic_frame.visible_effect_pass_count;
        self.graph.resolved_visible_effect_fbo_count = semantic_frame.visible_effect_fbo_count;
        Ok(&self.graph)
    }

    fn validate(
        &self,
        graph: &SceneRenderingDeviceGraphPlan,
        scene_time_seconds: f32,
    ) -> Result<(), String> {
        validate_topology_slice(
            "render-pass",
            &self.graph.pass_nodes,
            &graph.pass_nodes,
            scene_time_seconds,
        )?;
        validate_topology_slice(
            "render-target allocation",
            &self.graph.target_allocations,
            &graph.target_allocations,
            scene_time_seconds,
        )?;
        validate_topology_slice(
            "sampled binding",
            &self.graph.sampled_bindings,
            &graph.sampled_bindings,
            scene_time_seconds,
        )?;
        validate_topology_slice(
            "material sampled binding",
            &self.graph.material_sampled_bindings,
            &graph.material_sampled_bindings,
            scene_time_seconds,
        )?;
        let draws = graph
            .mesh_draws
            .iter()
            .map(draw_topology)
            .collect::<Vec<_>>();
        validate_topology_slice("mesh draw", &self.draws, &draws, scene_time_seconds)?;
        let palettes = graph
            .puppet_bone_palettes
            .iter()
            .map(|palette| SceneFramePuppetPaletteTopology {
                object: palette.object,
                puppet_index: palette.puppet_index,
                bone_matrix_start: palette.bone_matrix_start,
                bone_matrix_count: palette.bone_matrix_count,
            })
            .collect::<Vec<_>>();
        validate_topology_slice(
            "puppet palette",
            &self.palettes,
            &palettes,
            scene_time_seconds,
        )?;
        let bones = graph
            .puppet_bone_matrices
            .iter()
            .map(|bone| SceneFramePuppetBoneTopology {
                puppet_index: bone.puppet_index,
                bone_index: bone.bone_index,
                parent_index: bone.parent_index,
            })
            .collect::<Vec<_>>();
        validate_topology_slice("puppet bone", &self.bones, &bones, scene_time_seconds)
    }
}

fn validate_dynamic_counts(
    graph: &SceneRenderingDeviceGraphPlan,
    frame: &ResolvedSemanticFrame,
    scene_time_seconds: f32,
) -> Result<(), String> {
    let expected = [
        graph.resolved_object_count,
        graph.resolved_attachment_link_count,
    ];
    let actual = [frame.objects.len(), frame.attachment_links.len()];
    if expected != actual {
        return Err(format!(
            "scene semantic topology changed at {scene_time_seconds:.6}s: setup {expected:?}, frame {actual:?}; live topology mutation is not supported"
        ));
    }
    Ok(())
}

fn update_puppet_palettes(
    graph: &mut SceneRenderingDeviceGraphPlan,
    frame: &ResolvedSemanticFrame,
    scene_time_seconds: f32,
) -> Result<(), String> {
    if graph.puppet_bone_palettes.len() != frame.puppet_bone_palettes.len()
        || graph.puppet_bone_matrices.len() != frame.puppet_bone_matrices.len()
    {
        return Err(format!(
            "scene puppet topology changed at {scene_time_seconds:.6}s: setup palettes/bones {}/{}, frame {}/{}",
            graph.puppet_bone_palettes.len(),
            graph.puppet_bone_matrices.len(),
            frame.puppet_bone_palettes.len(),
            frame.puppet_bone_matrices.len()
        ));
    }
    for (target, source) in graph
        .puppet_bone_palettes
        .iter_mut()
        .zip(&frame.puppet_bone_palettes)
    {
        if (
            target.object,
            target.puppet_index,
            target.bone_matrix_start,
            target.bone_matrix_count,
        ) != (
            source.object,
            source.puppet_index,
            source.bone_start,
            source.bone_count,
        ) {
            return Err(format!(
                "scene puppet palette topology changed at {scene_time_seconds:.6}s for object {}",
                source.object.0
            ));
        }
        target.resolved_visible = source.resolved_visible;
    }
    for (target, source) in graph
        .puppet_bone_matrices
        .iter_mut()
        .zip(&frame.puppet_bone_matrices)
    {
        if (target.puppet_index, target.bone_index, target.parent_index)
            != (source.puppet_index, source.bone_index, source.parent_index)
        {
            return Err(format!(
                "scene puppet bone topology changed at {scene_time_seconds:.6}s for puppet {} bone {}",
                source.puppet_index, source.bone_index
            ));
        }
        target.matrix = rows_from_column_major(source.matrix);
        target.alpha = source.alpha;
    }
    Ok(())
}

fn rows_from_column_major(matrix: [f32; 16]) -> [[f32; 4]; 4] {
    [
        [matrix[0], matrix[4], matrix[8], matrix[12]],
        [matrix[1], matrix[5], matrix[9], matrix[13]],
        [matrix[2], matrix[6], matrix[10], matrix[14]],
        [matrix[3], matrix[7], matrix[11], matrix[15]],
    ]
}

fn update_draw_visibility(
    graph: &SceneRenderingDeviceGraphPlan,
    sampled_target_producers: &[SceneFrameSampledTargetProducerTopology],
    frame: &ResolvedSemanticFrame,
    draw_commands: &mut [SceneGpuDrawCommand],
) {
    for (draw, command) in graph.mesh_draws.iter().zip(draw_commands.iter_mut()) {
        command.enabled = draw.object.0 == crate::engine::scene::INVALID_OBJECT_ID
            || frame
                .object(draw.object)
                .is_some_and(|object| object.resolved_visible);
    }
    apply_effect_branch_visibility(graph, frame, draw_commands);
    disable_inactive_effect_gated_graph_draws(graph, frame, draw_commands);
    retain_live_sampled_target_producers(graph, sampled_target_producers, frame, draw_commands);
}

fn disable_unspawned_particle_draws(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    scene_time_seconds: f32,
    draw_commands: &mut [SceneGpuDrawCommand],
) {
    for (draw, command) in graph.mesh_draws.iter().zip(draw_commands) {
        if !command.enabled
            || draw.primitive != SceneRenderingDeviceDrawPrimitive::ParticleBillboard
        {
            continue;
        }
        let Some(particle) = storage.particle(draw.particle_index) else {
            command.enabled = false;
            continue;
        };
        if particle.parent_particle_index != crate::engine::scene::INVALID_PARTICLE_INDEX
            && particle_spawned_count(particle, scene_time_seconds) == 0
        {
            command.enabled = false;
        }
    }
}

fn particle_spawned_count(
    particle: &crate::engine::scene::SceneParticleSystemRecord,
    scene_time_seconds: f32,
) -> u32 {
    let now = scene_time_seconds * particle.instance_time_scale;
    let spawned = ((now + particle.start_time).max(0.0) * particle.rate.max(0.0)).floor();
    if spawned.is_finite() {
        spawned.clamp(0.0, u32::MAX as f32) as u32
    } else {
        0
    }
}

fn apply_effect_branch_visibility(
    graph: &SceneRenderingDeviceGraphPlan,
    frame: &ResolvedSemanticFrame,
    draw_commands: &mut [SceneGpuDrawCommand],
) {
    for pass in &graph.pass_nodes {
        if effect_branch_is_active(pass, frame) {
            continue;
        }
        let draw_start = pass.mesh_draw_start as usize;
        let draw_end = draw_start.saturating_add(pass.mesh_draw_count as usize);
        if let Some(commands) = draw_commands.get_mut(draw_start..draw_end) {
            commands
                .iter_mut()
                .for_each(|command| command.enabled = false);
        }
    }
}

fn retain_live_sampled_target_producers(
    graph: &SceneRenderingDeviceGraphPlan,
    sampled_target_producers: &[SceneFrameSampledTargetProducerTopology],
    frame: &ResolvedSemanticFrame,
    draw_commands: &mut [SceneGpuDrawCommand],
) {
    for (pass_node_index, pass) in graph.pass_nodes.iter().enumerate() {
        let pass_is_live = if pass.mesh_draw_count == 0 {
            render_graph_is_active(graph, pass.graph_index, frame)
        } else {
            pass_has_enabled_draw(pass, draw_commands)
        };
        if !pass_is_live {
            continue;
        }
        retain_pass_sampled_target_producers(
            graph,
            sampled_target_producers,
            frame,
            pass_node_index,
            draw_commands,
        );
    }
}

fn retain_pass_sampled_target_producers(
    graph: &SceneRenderingDeviceGraphPlan,
    sampled_target_producers: &[SceneFrameSampledTargetProducerTopology],
    frame: &ResolvedSemanticFrame,
    consumer_pass_node_index: usize,
    draw_commands: &mut [SceneGpuDrawCommand],
) {
    for dependency in sampled_target_producers.iter().filter(|dependency| {
        dependency.consumer_pass_node_index as usize == consumer_pass_node_index
    }) {
        let Some(producer_pass_node_index) = dependency
            .producer_pass_node_indices
            .iter()
            .rev()
            .map(|index| *index as usize)
            .find(|index| {
                let producer = &graph.pass_nodes[*index];
                render_graph_is_active(graph, producer.graph_index, frame)
                    && effect_branch_is_active(producer, frame)
            })
        else {
            continue;
        };
        let producer = &graph.pass_nodes[producer_pass_node_index];
        let draw_start = producer.mesh_draw_start as usize;
        let draw_end = draw_start.saturating_add(producer.mesh_draw_count as usize);
        if let Some(commands) = draw_commands.get_mut(draw_start..draw_end) {
            commands
                .iter_mut()
                .for_each(|command| command.enabled = true);
        }
        retain_pass_sampled_target_producers(
            graph,
            sampled_target_producers,
            frame,
            producer_pass_node_index,
            draw_commands,
        );
    }
}

fn sampled_target_producer_topology(
    graph: &SceneRenderingDeviceGraphPlan,
) -> Vec<SceneFrameSampledTargetProducerTopology> {
    graph
        .sampled_bindings
        .iter()
        .filter_map(|binding| {
            let consumer_pass_node_index = binding.pass_node_index as usize;
            let preceding_passes = graph.pass_nodes.get(..consumer_pass_node_index)?;
            let (producer_graph_index, target, target_name) = binding.logical_target()?;
            let target_is_graph_owned = graph.target_allocations.iter().any(|allocation| {
                allocation.graph_index == producer_graph_index
                    && allocation.target == target
                    && allocation.target_name == target_name
            });
            if !target_is_graph_owned {
                return None;
            }
            let producer_pass_node_indices = preceding_passes
                .iter()
                .enumerate()
                .filter_map(|(index, pass)| {
                    (pass.graph_index == producer_graph_index
                        && pass.target == target
                        && pass.target_name == target_name)
                        .then_some(index as u32)
                })
                .collect::<Vec<_>>();
            if producer_pass_node_indices.is_empty() {
                return None;
            }
            Some(SceneFrameSampledTargetProducerTopology {
                consumer_pass_node_index: binding.pass_node_index,
                producer_pass_node_indices,
            })
        })
        .collect()
}

fn effect_branch_is_active(
    pass: &crate::engine::scene::SceneRenderingDevicePassNode,
    frame: &ResolvedSemanticFrame,
) -> bool {
    let any_visible = || {
        (0..pass.effect_binding_count).any(|local_index| {
            frame
                .object_effect(pass.effect_binding_start.saturating_add(local_index))
                .is_some_and(|effect| effect.resolved_visible)
        })
    };
    match pass.effect_visibility_policy {
        crate::engine::scene::SceneRenderEffectVisibilityPolicy::AnyVisible => any_visible(),
        crate::engine::scene::SceneRenderEffectVisibilityPolicy::NoneVisible => !any_visible(),
        _ => true,
    }
}

fn pass_has_enabled_draw(
    pass: &crate::engine::scene::SceneRenderingDevicePassNode,
    draw_commands: &[SceneGpuDrawCommand],
) -> bool {
    let draw_start = pass.mesh_draw_start as usize;
    let draw_end = draw_start.saturating_add(pass.mesh_draw_count as usize);
    draw_commands
        .get(draw_start..draw_end)
        .is_some_and(|commands| commands.iter().any(|command| command.enabled))
}

fn render_graph_is_active(
    graph: &SceneRenderingDeviceGraphPlan,
    graph_index: u32,
    frame: &ResolvedSemanticFrame,
) -> bool {
    let Some(policy) = graph
        .pass_nodes
        .iter()
        .find(|pass| pass.graph_index == graph_index)
        .map(|pass| pass.graph_activation_policy)
    else {
        return false;
    };
    if policy == crate::engine::scene::SceneRenderGraphActivationPolicy::Always {
        return true;
    }
    graph
        .pass_nodes
        .iter()
        .filter(|pass| pass.graph_index == graph_index)
        .any(|pass| {
            (0..pass.effect_binding_count).any(|local_index| {
                frame
                    .object_effect(pass.effect_binding_start.saturating_add(local_index))
                    .is_some_and(|effect| effect.resolved_visible)
            })
        })
}

fn disable_inactive_effect_gated_graph_draws(
    graph: &SceneRenderingDeviceGraphPlan,
    frame: &ResolvedSemanticFrame,
    draw_commands: &mut [SceneGpuDrawCommand],
) {
    let mut graph_pass_start = 0;
    while graph_pass_start < graph.pass_nodes.len() {
        let graph_index = graph.pass_nodes[graph_pass_start].graph_index;
        let graph_pass_count = graph.pass_nodes[graph_pass_start..]
            .iter()
            .take_while(|pass| pass.graph_index == graph_index)
            .count();
        let graph_pass_end = graph_pass_start + graph_pass_count;
        let graph_passes = &graph.pass_nodes[graph_pass_start..graph_pass_end];
        debug_assert!(graph_passes.iter().all(|pass| {
            pass.graph_activation_policy == graph_passes[0].graph_activation_policy
        }));
        if !render_graph_is_active(graph, graph_index, frame) {
            for pass in graph_passes {
                let draw_start = pass.mesh_draw_start as usize;
                let draw_end = draw_start.saturating_add(pass.mesh_draw_count as usize);
                if let Some(commands) = draw_commands.get_mut(draw_start..draw_end) {
                    commands
                        .iter_mut()
                        .for_each(|command| command.enabled = false);
                }
            }
        }
        graph_pass_start = graph_pass_end;
    }
}

fn update_effect_draw_pipelines(
    graph: &SceneRenderingDeviceGraphPlan,
    draw_commands: &mut [SceneGpuDrawCommand],
) -> Result<(), String> {
    for (draw, command) in graph.mesh_draws.iter().zip(draw_commands) {
        command.pipeline_index = command.authored_pipeline_index;
        if draw.effect_visibility_policy
            != crate::engine::scene::SceneRenderEffectVisibilityPolicy::Passthrough
            || draw.resolved_effect_visibility_mask & 1 != 0
        {
            continue;
        }
        command.pipeline_index = command.disabled_pipeline_index.ok_or_else(|| {
            format!(
                "scene effect binding {} has passthrough visibility policy but no disabled pipeline",
                draw.effect_binding_start
            )
        })?;
    }
    Ok(())
}

fn elapsed_optional_micros(started: Option<Instant>) -> u64 {
    started
        .map(|started| started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

fn draw_topology(draw: &SceneRenderingDeviceMeshDraw) -> SceneFrameDrawTopology {
    SceneFrameDrawTopology {
        primitive: draw.primitive,
        projection_domain: draw.projection_domain,
        mesh_index: draw.mesh_index,
        resolved_object_index: draw.resolved_object_index,
        effect_binding_start: draw.effect_binding_start,
        effect_binding_count: draw.effect_binding_count,
        effect_visibility_policy: draw.effect_visibility_policy,
        skinning_palette_start: draw.skinning_palette_start,
        skinning_palette_count: draw.skinning_palette_count,
        object: draw.object,
        material: draw.material,
        vertex_start: draw.vertex_start,
        vertex_count: draw.vertex_count,
        index_start: draw.index_start,
        index_count: draw.index_count,
    }
}

#[cfg(test)]
#[path = "frame_state/tests.rs"]
mod tests;
