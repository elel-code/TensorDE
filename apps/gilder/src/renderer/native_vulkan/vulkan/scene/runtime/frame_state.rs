//! Dynamic semantic-frame planning and host-visible GPU buffer updates.

use std::time::Instant;

use vulkanalia::prelude::v1_4::*;

use crate::engine::scene::rendering_device_graph::scene_clip_transform;
use crate::engine::scene::semantic_world::{ResolvedSemanticFrame, SemanticFrameResolver};
use crate::engine::scene::{
    SceneFrameEvents, SceneMaterialHandle, SceneObjectHandle, SceneRenderingDeviceDrawPrimitive,
    SceneRenderingDeviceGraphPlan, SceneRenderingDeviceMeshDraw, SceneSemanticWorld, SceneStorage,
};
use crate::renderer::native_vulkan::{
    NativeVulkanVulkanaliaBuffer, native_vulkan_vulkanalia_write_host_buffer,
};

use super::composite_scissor::SceneMeshCoveragePlans;
use super::composite_scissor::update_scene_composite_scissors;
use super::draw_recording::SceneGpuDrawCommand;
use super::draw_uniform::pack_scene_draw_uniforms_into;
use super::dynamic_text::SceneDynamicTextRuntime;
use super::material_uniform::{
    SceneMaterialFrameInputs, pack_scene_material_uniforms_with_frame_inputs,
};
use super::scene_color_clear::{SceneGpuSceneColorClear, resolve_scene_color_attachment_clear};
use super::scene_owned_uniform::{SceneOwnedUniformArenaPlan, SceneOwnedUniformFrameInputs};

mod topology;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SceneFrameSampledTargetProducerTopology {
    consumer_pass_node_index: u32,
    producer_pass_node_index: u32,
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
            draw.clip_transform =
                scene_clip_transform(storage.project(), object.render_world_matrix);
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

pub(super) fn write_scene_frame_buffers(
    device: &Device,
    storage: &SceneStorage,
    mesh_coverage: &SceneMeshCoveragePlans,
    semantic_world: &SceneSemanticWorld<'_>,
    semantic_resolver: &mut SemanticFrameResolver,
    topology: &mut SceneFrameTopology,
    draw_commands: &mut [SceneGpuDrawCommand],
    transform_scratch: &mut Vec<u8>,
    transform_buffer: &NativeVulkanVulkanaliaBuffer,
    material_buffer: Option<&NativeVulkanVulkanaliaBuffer>,
    skinning_buffer: Option<&NativeVulkanVulkanaliaBuffer>,
    scene_owned_uniform_plan: &SceneOwnedUniformArenaPlan,
    scene_owned_uniform_scratch: &mut [u8],
    scene_owned_uniform_buffer: Option<&NativeVulkanVulkanaliaBuffer>,
    sampled_binding_phase: usize,
    dynamic_effect_uniforms: bool,
    cpu_timing_enabled: bool,
    graph_execution_order: &[u32],
    scene_color_attachment_clear_enabled: bool,
    events: &SceneFrameEvents,
    scene_time_seconds: f32,
    frame_delta_seconds: f32,
    output_extent: [u32; 2],
    dynamic_text: &mut SceneDynamicTextRuntime,
) -> Result<SceneFrameBufferUpdate, String> {
    let semantic_started = cpu_timing_enabled.then(Instant::now);
    let semantic_frame = semantic_resolver
        .resolve_frame_with_events_at(
            semantic_world,
            scene_time_seconds,
            frame_delta_seconds,
            events,
        )
        .map_err(|err| {
            format!(
                "resolve scene semantic frame at {scene_time_seconds:.6}s for Vulkan buffer update: {err}"
            )
        })?;
    let semantic_resolve_micros = elapsed_optional_micros(semantic_started);
    let graph_started = cpu_timing_enabled.then(Instant::now);
    topology.update_dynamic_graph(storage, &semantic_frame, scene_time_seconds)?;
    update_draw_visibility(
        &topology.graph,
        &topology.sampled_target_producers,
        semantic_frame,
        draw_commands,
    );
    let graph = &topology.graph;
    update_effect_draw_pipelines(graph, draw_commands)?;
    let graph_update_micros = elapsed_optional_micros(graph_started);

    let transform_started = cpu_timing_enabled.then(Instant::now);
    pack_scene_draw_uniforms_into(
        transform_scratch,
        storage,
        &graph.mesh_draws,
        scene_time_seconds,
        output_extent,
    );
    let mut dynamic_text_instance_updated = false;
    if !dynamic_text.is_empty() {
        let (changed, instances, states) = dynamic_text.update(semantic_frame)?;
        dynamic_text_instance_updated = changed;
        for (draw, command) in graph.mesh_draws.iter().zip(draw_commands.iter_mut()) {
            if !command.dynamic_text {
                continue;
            }
            let state = states
                .iter()
                .find(|state| state.object == draw.object)
                .ok_or_else(|| {
                    format!(
                        "dynamic text draw object {} has no retained layout",
                        draw.object.0
                    )
                })?;
            command.first_instance = state.first_instance;
            command.instance_count = state.instance_count;
        }
        transform_scratch.extend_from_slice(instances);
    }
    write_exact_frame_payload(device, transform_buffer, transform_scratch)?;
    let transform_update_micros = elapsed_optional_micros(transform_started);

    let material_started = cpu_timing_enabled.then(Instant::now);
    let material_uniform_updated = if dynamic_effect_uniforms {
        let material_buffer = material_buffer.ok_or_else(|| {
            "scene has dynamic effect uniforms but no material uniform buffer".to_owned()
        })?;
        let stereo_spectrum64 = events.audio_spectrum();
        let average_spectrum32 = stereo_spectrum64.map(|spectrum| spectrum.average32());
        let material_payload = pack_scene_material_uniforms_with_frame_inputs(
            storage,
            &graph.mesh_draws,
            scene_time_seconds,
            output_extent,
            SceneMaterialFrameInputs {
                average_spectrum32: average_spectrum32.as_ref(),
                audio_material_values: &semantic_frame.audio_band_material_values,
                material_scalar_values: &semantic_frame.material_scalar_values,
            },
        );
        write_exact_frame_payload(device, material_buffer, &material_payload)?;
        true
    } else {
        false
    };
    let material_update_micros = elapsed_optional_micros(material_started);

    let scene_owned_uniform_started = cpu_timing_enabled.then(Instant::now);
    let scene_owned_uniform_updated = if scene_owned_uniform_plan.is_empty() {
        false
    } else {
        let buffer = scene_owned_uniform_buffer
            .ok_or_else(|| "scene-owned uniform plan has no active frame buffer".to_owned())?;
        scene_owned_uniform_plan.write_payload(
            &graph.mesh_draws,
            SceneOwnedUniformFrameInputs {
                scalar_overrides: &semantic_frame.material_scalar_values,
                scene_time_seconds,
                frame_delta_seconds,
                audio_spectrum: events
                    .audio_spectrum()
                    .unwrap_or(&crate::engine::scene::StereoSpectrum64::ZERO),
                sampled_binding_phase,
            },
            scene_owned_uniform_scratch,
        )?;
        write_exact_frame_payload(device, buffer, scene_owned_uniform_scratch)?;
        true
    };
    let scene_owned_uniform_update_micros = elapsed_optional_micros(scene_owned_uniform_started);

    let skinning_started = cpu_timing_enabled.then(Instant::now);
    let skinning_storage_updated = if let Some(skinning_buffer) = skinning_buffer {
        let skinning_payload = pack_scene_skinning_palette(&graph);
        write_exact_frame_payload(device, skinning_buffer, &skinning_payload)?;
        true
    } else {
        false
    };
    let skinning_update_micros = elapsed_optional_micros(skinning_started);

    let draw_policy_started = cpu_timing_enabled.then(Instant::now);
    update_scene_composite_scissors(storage, mesh_coverage, graph, output_extent, draw_commands)?;
    let scene_color_attachment_clear = resolve_scene_color_attachment_clear(
        storage,
        mesh_coverage,
        graph,
        graph_execution_order,
        output_extent,
        scene_color_attachment_clear_enabled,
    );
    let draw_policy_update_micros = elapsed_optional_micros(draw_policy_started);

    Ok(SceneFrameBufferUpdate {
        transform_uniform_updated: true,
        material_uniform_updated,
        skinning_storage_updated,
        scene_owned_uniform_updated,
        dynamic_text_instance_updated,
        scene_color_attachment_clear,
        cpu_timing: SceneFrameCpuTiming {
            semantic_resolve_micros,
            graph_update_micros,
            transform_update_micros,
            material_update_micros,
            skinning_update_micros,
            scene_owned_uniform_update_micros,
            draw_policy_update_micros,
        },
    })
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
    disable_inactive_effect_gated_graph_draws(graph, frame, draw_commands);
    retain_live_sampled_target_producers(graph, sampled_target_producers, frame, draw_commands);
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
        let producer_pass_node_index = dependency.producer_pass_node_index as usize;
        let producer = &graph.pass_nodes[producer_pass_node_index];
        if !render_graph_is_active(graph, producer.graph_index, frame) {
            continue;
        }
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
            let producer_pass_node_index = preceding_passes.iter().rposition(|pass| {
                pass.graph_index == producer_graph_index
                    && pass.target == target
                    && pass.target_name == target_name
            })?;
            Some(SceneFrameSampledTargetProducerTopology {
                consumer_pass_node_index: binding.pass_node_index,
                producer_pass_node_index: producer_pass_node_index as u32,
            })
        })
        .collect()
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

fn write_exact_frame_payload(
    device: &Device,
    buffer: &NativeVulkanVulkanaliaBuffer,
    payload: &[u8],
) -> Result<(), String> {
    if payload.len() as u64 != buffer.snapshot.requested_bytes {
        return Err(format!(
            "{} frame payload has {} bytes, but the setup allocation has {} bytes",
            buffer.snapshot.role,
            payload.len(),
            buffer.snapshot.requested_bytes
        ));
    }
    native_vulkan_vulkanalia_write_host_buffer(device, buffer, payload)
}

#[cfg(test)]
#[path = "frame_state/tests.rs"]
mod tests;
