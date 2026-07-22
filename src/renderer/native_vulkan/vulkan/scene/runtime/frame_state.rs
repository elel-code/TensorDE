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

use super::composite_scissor::update_scene_composite_scissors;
use super::composite_scissor::SceneMeshCoveragePlans;
use super::draw_recording::SceneGpuDrawCommand;
use super::draw_uniform::pack_scene_draw_uniforms;
use super::material_uniform::pack_scene_material_uniforms_with_frame_inputs;
use super::scene_color_clear::{SceneGpuSceneColorClear, resolve_scene_color_attachment_clear};

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
    pub draw_policy_update_micros: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct SceneFrameTopology {
    graph: SceneRenderingDeviceGraphPlan,
    draws: Vec<SceneFrameDrawTopology>,
    palettes: Vec<SceneFramePuppetPaletteTopology>,
    bones: Vec<SceneFramePuppetBoneTopology>,
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

impl SceneFrameTopology {
    pub(super) fn from_graph(graph: &SceneRenderingDeviceGraphPlan) -> Self {
        Self::from_owned_graph(graph.clone())
    }

    pub(super) fn from_owned_graph(graph: SceneRenderingDeviceGraphPlan) -> Self {
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
            draw.clip_transform =
                scene_clip_transform(storage.project(), object.render_world_matrix);
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
    let actual = [
        frame.objects.len(),
        frame.attachment_links.len(),
    ];
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
    transform_buffer: &NativeVulkanVulkanaliaBuffer,
    material_buffer: Option<&NativeVulkanVulkanaliaBuffer>,
    skinning_buffer: Option<&NativeVulkanVulkanaliaBuffer>,
    dynamic_effect_uniforms: bool,
    cpu_timing_enabled: bool,
    graph_execution_order: &[u32],
    scene_color_attachment_clear_enabled: bool,
    events: &SceneFrameEvents,
    scene_time_seconds: f32,
    output_extent: [u32; 2],
) -> Result<SceneFrameBufferUpdate, String> {
    let semantic_started = cpu_timing_enabled.then(Instant::now);
    let semantic_frame = semantic_resolver
        .resolve_frame_with_events_at(semantic_world, scene_time_seconds, events)
        .map_err(|err| {
            format!(
                "resolve scene semantic frame at {scene_time_seconds:.6}s for Vulkan buffer update: {err}"
            )
        })?;
    let semantic_resolve_micros = elapsed_optional_micros(semantic_started);
    let graph_started = cpu_timing_enabled.then(Instant::now);
    let graph = topology.update_dynamic_graph(storage, &semantic_frame, scene_time_seconds)?;
    update_draw_visibility(graph, semantic_frame, draw_commands);
    update_effect_draw_pipelines(graph, draw_commands)?;
    let graph_update_micros = elapsed_optional_micros(graph_started);

    let transform_started = cpu_timing_enabled.then(Instant::now);
    let transform_payload = pack_scene_draw_uniforms(
        storage,
        &graph.mesh_draws,
        scene_time_seconds,
        output_extent,
    );
    write_exact_frame_payload(device, transform_buffer, &transform_payload)?;
    let transform_update_micros = elapsed_optional_micros(transform_started);

    let material_started = cpu_timing_enabled.then(Instant::now);
    let material_uniform_updated = if dynamic_effect_uniforms {
        let material_buffer = material_buffer.ok_or_else(|| {
            "scene has dynamic effect uniforms but no material uniform buffer".to_owned()
        })?;
        let material_payload = pack_scene_material_uniforms_with_frame_inputs(
            storage,
            &graph.mesh_draws,
            scene_time_seconds,
            events.audio_spectrum(),
            &semantic_frame.audio_band_material_values,
        );
        write_exact_frame_payload(device, material_buffer, &material_payload)?;
        true
    } else {
        false
    };
    let material_update_micros = elapsed_optional_micros(material_started);

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
    update_scene_composite_scissors(
        storage,
        mesh_coverage,
        graph,
        output_extent,
        draw_commands,
    )?;
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
        scene_color_attachment_clear,
        cpu_timing: SceneFrameCpuTiming {
            semantic_resolve_micros,
            graph_update_micros,
            transform_update_micros,
            material_update_micros,
            skinning_update_micros,
            draw_policy_update_micros,
        },
    })
}

fn update_draw_visibility(
    graph: &SceneRenderingDeviceGraphPlan,
    frame: &ResolvedSemanticFrame,
    draw_commands: &mut [SceneGpuDrawCommand],
) {
    for (draw, command) in graph
        .mesh_draws
        .iter()
        .zip(draw_commands.iter_mut())
    {
        command.enabled = draw.object.0 == crate::engine::scene::INVALID_OBJECT_ID
            || frame
                .object(draw.object)
                .is_some_and(|object| object.resolved_visible);
    }
    disable_inactive_effect_gated_graph_draws(graph, frame, draw_commands);
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
        let effect_gated = graph_passes[0].graph_activation_policy
            == crate::engine::scene::SceneRenderGraphActivationPolicy::AnyEffectVisible;
        debug_assert!(graph_passes.iter().all(|pass| {
            pass.graph_activation_policy == graph_passes[0].graph_activation_policy
        }));
        if effect_gated {
            let any_effect_visible = graph_passes.iter().any(|pass| {
                (0..pass.effect_binding_count).any(|local_index| {
                    frame
                        .object_effect(pass.effect_binding_start.saturating_add(local_index))
                        .is_some_and(|effect| effect.resolved_visible)
                })
            });
            if !any_effect_visible {
                for pass in graph_passes {
                    let draw_start = pass.mesh_draw_start as usize;
                    let draw_end = draw_start.saturating_add(pass.mesh_draw_count as usize);
                    if let Some(commands) = draw_commands.get_mut(draw_start..draw_end) {
                        commands.iter_mut().for_each(|command| command.enabled = false);
                    }
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
mod tests {
    use super::*;
    use crate::engine::scene::semantic_world::{
        ResolvedObjectEffectState, ResolvedPuppetBoneMatrix, ResolvedPuppetBonePalette,
        SemanticEntity,
    };
    use crate::engine::scene::{
        SceneEffectHandle, SceneMaterialHandle, SceneRenderGraphActivationPolicy,
        SceneRenderPassKind, SceneRenderTargetKind, SceneRenderingDevicePassNode,
        SceneRenderingDevicePuppetBoneMatrix, SceneRenderingDevicePuppetBonePalette, SceneStringId,
    };
    use crate::renderer::native_vulkan::NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES;

    #[test]
    fn hidden_passthrough_effect_switches_pipeline_without_affecting_material_stage_draw() {
        let mut graph = graph_with_bone(SceneRenderingDevicePuppetBoneMatrix {
            puppet_index: 0,
            bone_index: 0,
            parent_index: -1,
            matrix: [[0.0; 4]; 4],
            alpha: 1.0,
        });
        graph.mesh_draws = vec![
            effect_draw(
                crate::engine::scene::SceneRenderEffectVisibilityPolicy::Passthrough,
                0,
                3,
            ),
            effect_draw(
                crate::engine::scene::SceneRenderEffectVisibilityPolicy::MaterialStages,
                0,
                4,
            ),
        ];
        let mut commands = vec![draw_command(10, Some(20)), draw_command(11, None)];

        update_effect_draw_pipelines(&graph, &mut commands).expect("typed visibility pipelines");

        assert_eq!(commands[0].pipeline_index, 20);
        assert_eq!(commands[1].pipeline_index, 11);

        graph.mesh_draws[0].resolved_effect_visibility_mask = 1;
        update_effect_draw_pipelines(&graph, &mut commands).expect("visible authored pipeline");
        assert_eq!(commands[0].pipeline_index, 10);
    }

    #[test]
    fn effect_only_framebuffer_graph_disables_every_draw_when_all_effects_are_hidden() {
        let mut graph = graph_with_bone(SceneRenderingDevicePuppetBoneMatrix {
            puppet_index: 0,
            bone_index: 0,
            parent_index: -1,
            matrix: [[0.0; 4]; 4],
            alpha: 1.0,
        });
        let pass = |pass_id,
                    role,
                    effect_binding_start,
                    effect_binding_count,
                    effect_visibility_policy,
                    mesh_draw_start| SceneRenderingDevicePassNode {
            graph_index: 4,
            graph_activation_policy: SceneRenderGraphActivationPolicy::AnyEffectVisible,
            pass_record_index: pass_id,
            pass_id,
            role,
            target: SceneRenderTargetKind::SceneColor,
            target_name: SceneStringId::NONE,
            binding_start: 0,
            binding_count: 0,
            effect_binding_start,
            effect_binding_count,
            effect_visibility_policy,
            mesh_draw_start,
            mesh_draw_count: 1,
        };
        graph.pass_nodes = vec![
            pass(
                0,
                SceneRenderPassKind::BaseMaterial,
                u32::MAX,
                0,
                crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
                0,
            ),
            pass(
                1,
                SceneRenderPassKind::EffectMaterial,
                0,
                1,
                crate::engine::scene::SceneRenderEffectVisibilityPolicy::Passthrough,
                1,
            ),
            pass(
                2,
                SceneRenderPassKind::SceneComposite,
                u32::MAX,
                0,
                crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
                2,
            ),
        ];
        graph.mesh_draws = vec![
            effect_draw(
                crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
                0,
                u32::MAX,
            ),
            effect_draw(
                crate::engine::scene::SceneRenderEffectVisibilityPolicy::Passthrough,
                0,
                0,
            ),
            effect_draw(
                crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
                0,
                u32::MAX,
            ),
        ];
        let mut commands = vec![
            draw_command(10, None),
            draw_command(11, Some(21)),
            draw_command(12, None),
        ];
        let mut frame = frame_with_effect_visibility(false);

        update_draw_visibility(&graph, &frame, &mut commands);
        assert!(commands.iter().all(|command| !command.enabled));

        frame.object_effects[0].resolved_visible = true;
        update_draw_visibility(&graph, &frame, &mut commands);
        assert!(commands.iter().all(|command| command.enabled));
    }

    #[test]
    fn skinning_payload_prefixes_identity_and_packs_alpha_in_std430_entry() {
        let graph = graph_with_bone(SceneRenderingDevicePuppetBoneMatrix {
            puppet_index: 0,
            bone_index: 41,
            parent_index: -1,
            matrix: [
                [1.0, 2.0, 3.0, 4.0],
                [5.0, 6.0, 7.0, 8.0],
                [9.0, 10.0, 11.0, 12.0],
                [13.0, 14.0, 15.0, 16.0],
            ],
            alpha: 0.375,
        });

        let payload = pack_scene_skinning_palette(&graph);

        assert_eq!(
            payload.len(),
            2 * NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES
        );
        assert_eq!(payload_f32(&payload, 0), 1.0);
        assert_eq!(payload_f32(&payload, 60), 1.0);
        assert_eq!(payload_f32(&payload, 64), 1.0);
        assert_eq!(
            payload_f32(
                &payload,
                NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES
            ),
            1.0
        );
        assert_eq!(
            payload_f32(
                &payload,
                NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES + 60
            ),
            16.0
        );
        assert_eq!(
            payload_f32(
                &payload,
                NATIVE_VULKAN_SCENE_PUPPET_BONE_PALETTE_ENTRY_BYTES + 64
            ),
            0.375
        );
    }

    #[test]
    fn topology_ignores_dynamic_matrix_and_alpha_values() {
        let setup = graph_with_bone(SceneRenderingDevicePuppetBoneMatrix {
            puppet_index: 0,
            bone_index: 41,
            parent_index: -1,
            matrix: [[0.0; 4]; 4],
            alpha: 1.0,
        });
        let mut frame = setup.clone();
        frame.puppet_bone_matrices[0].matrix = [[2.0; 4]; 4];
        frame.puppet_bone_matrices[0].alpha = 0.25;

        SceneFrameTopology::from_graph(&setup)
            .validate(&frame, 1.0)
            .expect("dynamic bone values preserve topology");
    }

    #[test]
    fn topology_rejects_dynamic_bone_reordering() {
        let setup = graph_with_bone(SceneRenderingDevicePuppetBoneMatrix {
            puppet_index: 0,
            bone_index: 41,
            parent_index: -1,
            matrix: [[0.0; 4]; 4],
            alpha: 1.0,
        });
        let mut frame = setup.clone();
        frame.puppet_bone_matrices[0].bone_index = 42;

        let error = SceneFrameTopology::from_graph(&setup)
            .validate(&frame, 1.0)
            .unwrap_err();
        assert!(error.contains("puppet bone topology changed"));
        assert!(error.contains("index 0"));
    }

    #[test]
    fn retained_graph_updates_dynamic_palette_matrix_and_alpha_in_place() {
        let mut graph = graph_with_bone(SceneRenderingDevicePuppetBoneMatrix {
            puppet_index: 0,
            bone_index: 41,
            parent_index: -1,
            matrix: [[0.0; 4]; 4],
            alpha: 1.0,
        });
        let matrix = [
            1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0,
        ];
        let frame = ResolvedSemanticFrame {
            objects: Vec::new(),
            object_effects: Vec::new(),
            attachment_links: Vec::new(),
            puppet_bone_palettes: vec![ResolvedPuppetBonePalette {
                object: SceneObjectHandle(0),
                puppet_index: 0,
                bone_start: 0,
                bone_count: 1,
                resolved_visible: true,
            }],
            puppet_bone_matrices: vec![ResolvedPuppetBoneMatrix {
                puppet_index: 0,
                bone_index: 41,
                parent_index: -1,
                matrix,
                alpha: 0.25,
            }],
            audio_band_material_values: Vec::new(),
            script_text_values: Vec::new(),
            media_clock: None,
            video_frame: None,
            visible_object_count: 0,
            visible_mesh_binding_count: 0,
            visible_effect_instance_count: 0,
            visible_effect_pass_count: 0,
            visible_effect_fbo_count: 0,
            visible_puppet_binding_count: 0,
            visible_puppet_bone_matrix_count: 1,
        };

        update_puppet_palettes(&mut graph, &frame, 2.0).expect("stable palette topology");

        assert_eq!(
            graph.puppet_bone_matrices[0].matrix,
            [
                [1.0, 5.0, 9.0, 13.0],
                [2.0, 6.0, 10.0, 14.0],
                [3.0, 7.0, 11.0, 15.0],
                [4.0, 8.0, 12.0, 16.0],
            ]
        );
        assert_eq!(graph.puppet_bone_matrices[0].alpha, 0.25);
    }

    fn graph_with_bone(
        bone: SceneRenderingDevicePuppetBoneMatrix,
    ) -> SceneRenderingDeviceGraphPlan {
        SceneRenderingDeviceGraphPlan {
            pass_nodes: Vec::new(),
            target_allocations: Vec::new(),
            effect_batches: Vec::new(),
            effect_batch_instances: Vec::new(),
            sampled_bindings: Vec::new(),
            material_sampled_bindings: Vec::new(),
            mesh_draws: Vec::new(),
            puppet_bone_palettes: vec![SceneRenderingDevicePuppetBonePalette {
                object: SceneObjectHandle(0),
                puppet_index: 0,
                bone_matrix_start: 0,
                bone_matrix_count: 1,
                resolved_visible: true,
            }],
            puppet_bone_matrices: vec![bone],
            particle_gpu_emitters: Vec::new(),
            resolved_object_count: 1,
            resolved_visible_object_count: 1,
            resolved_attachment_link_count: 0,
            resolved_visible_effect_instance_count: 0,
            resolved_visible_effect_pass_count: 0,
            resolved_visible_effect_fbo_count: 0,
            descriptor_heap_required: true,
            descriptor_heap_resource_count: 1,
            descriptor_heap_sampled_image_count: 0,
            descriptor_heap_uniform_buffer_count: 0,
            descriptor_heap_storage_buffer_count: 1,
            descriptor_heap_sampler_count: 0,
            graph_physical_target_count: 0,
            graph_aliased_target_count: 0,
            fifo_latest_ready_present_required: true,
        }
    }

    fn frame_with_effect_visibility(resolved_visible: bool) -> ResolvedSemanticFrame {
        ResolvedSemanticFrame {
            objects: Vec::new(),
            object_effects: vec![ResolvedObjectEffectState {
                binding_index: 0,
                entity: SemanticEntity::from_raw(0),
                object: SceneObjectHandle(0),
                object_index: 0,
                effect: SceneEffectHandle(0),
                effect_index: 0,
                instance_id: 0,
                self_visible: resolved_visible,
                object_resolved_visible: true,
                resolved_visible,
                pass_start: 0,
                pass_count: 1,
                fbo_start: 0,
                fbo_count: 0,
            }],
            attachment_links: Vec::new(),
            puppet_bone_palettes: Vec::new(),
            puppet_bone_matrices: Vec::new(),
            audio_band_material_values: Vec::new(),
            script_text_values: Vec::new(),
            media_clock: None,
            video_frame: None,
            visible_object_count: 0,
            visible_mesh_binding_count: 0,
            visible_effect_instance_count: usize::from(resolved_visible),
            visible_effect_pass_count: usize::from(resolved_visible),
            visible_effect_fbo_count: 0,
            visible_puppet_binding_count: 0,
            visible_puppet_bone_matrix_count: 0,
        }
    }

    fn effect_draw(
        policy: crate::engine::scene::SceneRenderEffectVisibilityPolicy,
        visibility_mask: u32,
        binding_start: u32,
    ) -> SceneRenderingDeviceMeshDraw {
        SceneRenderingDeviceMeshDraw {
            primitive: SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
            shader_key: crate::engine::scene::SceneStringId::NONE,
            mesh_index: crate::engine::scene::INVALID_OBJECT_ID,
            resolved_object_index: crate::engine::scene::INVALID_OBJECT_ID,
            clip_transform: [[0.0; 4]; 4],
            authored_source_extent: [1.0; 2],
            skinning_palette_start: crate::engine::scene::INVALID_OBJECT_ID,
            skinning_palette_count: 0,
            resolved_color: crate::engine::scene::SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            resolved_alpha: 1.0,
            apply_resolved_visual: false,
            effect_batch_atlas_tile: crate::engine::scene::INVALID_OBJECT_ID,
            effect_batch_atlas_grid: [0; 2],
            effect_binding_start: binding_start,
            effect_binding_count: 1,
            effect_visibility_policy: policy,
            resolved_effect_visibility_mask: visibility_mask,
            object: SceneObjectHandle(crate::engine::scene::INVALID_OBJECT_ID),
            material: SceneMaterialHandle(crate::engine::scene::INVALID_MATERIAL_ID),
            vertex_start: 0,
            vertex_count: 3,
            index_start: 0,
            index_count: 3,
            instance_count: 1,
        }
    }

    fn draw_command(
        authored_pipeline_index: u32,
        disabled_pipeline_index: Option<u32>,
    ) -> SceneGpuDrawCommand {
        SceneGpuDrawCommand {
            enabled: true,
            primitive: SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
            pipeline_index: authored_pipeline_index,
            authored_pipeline_index,
            disabled_pipeline_index,
            first_index: 0,
            index_count: 3,
            vertex_offset: 0,
            vertex_count: 3,
            instance_count: 1,
            instance_capacity: 1,
            particle_indirect_index: None,
            resource_descriptor_base: 0,
            sampler_descriptor_base: 0,
            skinning_byte_offset: 0,
            skinning_byte_count: 0,
            scissor: None,
            alpha_coverage_scissors: Vec::new(),
        }
    }

    fn payload_f32(payload: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap())
    }
}
