//! Semantic-frame updates for the shared retained scene owner.

use std::time::Instant;

use crate::engine::scene::semantic_world::SemanticFrameResolver;
use crate::engine::scene::{SceneFrameEvents, SceneSemanticWorld, SceneStorage};

use super::super::draw_uniform::pack_scene_draw_uniforms_into;
use super::super::material_uniform::{
    SceneMaterialFrameInputs, pack_scene_material_uniforms_with_frame_inputs_into,
};
use super::super::scene_owned_uniform::SceneOwnedUniformFrameInputs;
use super::super::shared_resources::particle_frame_state;
use super::super::shared_scene::SharedSceneGpuResources;
use super::super::{composite_scissor, scene_color_clear};
use super::topology::pack_scene_skinning_palette_into;
use super::video::pack_scene_video_vertices_into;
use super::{
    SceneFrameBufferUpdate, SceneFrameCpuTiming, disable_unspawned_particle_draws,
    elapsed_optional_micros, update_draw_visibility, update_effect_draw_pipelines,
};

impl SharedSceneGpuResources {
    #[allow(clippy::too_many_arguments)]
    pub(in crate::renderer::rendering_device::scene_present::scene::runtime) fn update_semantic_frame(
        &mut self,
        storage: &SceneStorage,
        semantic_world: &SceneSemanticWorld<'_>,
        semantic_resolver: &mut SemanticFrameResolver,
        frame_slot: usize,
        reference_phase: usize,
        cpu_timing_enabled: bool,
        events: &SceneFrameEvents,
        scene_time_seconds: f32,
        frame_delta_seconds: f32,
        output_extent: [u32; 2],
    ) -> Result<SceneFrameBufferUpdate, String> {
        let frame = self
            .frames
            .get(frame_slot)
            .ok_or_else(|| format!("shared scene frame slot {frame_slot} is missing"))?;
        frame.descriptor_phase(reference_phase)?;

        let semantic_started = cpu_timing_enabled.then(Instant::now);
        let semantic_frame = semantic_resolver
            .resolve_frame_with_events_at(
                semantic_world,
                scene_time_seconds,
                frame_delta_seconds,
                events,
            )
            .map_err(|error| {
                format!("resolve shared scene semantic frame at {scene_time_seconds:.6}s: {error}")
            })?;
        let semantic_resolve_micros = elapsed_optional_micros(semantic_started);

        let graph_started = cpu_timing_enabled.then(Instant::now);
        self.frame_topology
            .update_dynamic_graph(storage, semantic_frame, scene_time_seconds)?;
        let graph = self.frame_topology.graph();
        self.particle_frame_scratch =
            particle_frame_state(graph, scene_time_seconds, frame_delta_seconds)?;
        update_draw_visibility(
            graph,
            &self.frame_topology.sampled_target_producers,
            semantic_frame,
            &mut self.draw_commands,
        );
        disable_unspawned_particle_draws(
            storage,
            graph,
            scene_time_seconds,
            &mut self.draw_commands,
        );
        update_effect_draw_pipelines(graph, &mut self.draw_commands)?;
        let graph_update_micros = elapsed_optional_micros(graph_started);

        if !self.video_vertex_scratch.is_empty() {
            pack_scene_video_vertices_into(
                &mut self.video_vertex_scratch,
                storage,
                graph,
                &self.draw_commands,
                output_extent,
            )?;
            frame.write_video_vertex_payload(&self.video_vertex_scratch)?;
        }

        let transform_started = cpu_timing_enabled.then(Instant::now);
        pack_scene_draw_uniforms_into(
            &mut self.transform_scratch,
            storage,
            &graph.mesh_draws,
            scene_time_seconds,
            output_extent,
        );
        let mut dynamic_text_instance_updated = false;
        if !self.dynamic_text.is_empty() {
            let (changed, instances, states) = self.dynamic_text.update(semantic_frame)?;
            dynamic_text_instance_updated = changed;
            for (draw, command) in graph.mesh_draws.iter().zip(&mut self.draw_commands) {
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
            self.transform_scratch.extend_from_slice(instances);
        }
        frame.write_transform_payload(&self.transform_scratch)?;
        let transform_update_micros = elapsed_optional_micros(transform_started);

        let material_started = cpu_timing_enabled.then(Instant::now);
        let material_uniform_updated = if self.dynamic_effect_uniforms {
            let scratch = self.material_scratch.as_mut().ok_or_else(|| {
                "scene has dynamic effect uniforms but no material scratch".to_owned()
            })?;
            let stereo_spectrum64 = events.audio_spectrum();
            let average_spectrum32 = stereo_spectrum64.map(|spectrum| spectrum.average32());
            pack_scene_material_uniforms_with_frame_inputs_into(
                scratch,
                storage,
                &graph.mesh_draws,
                scene_time_seconds,
                output_extent,
                SceneMaterialFrameInputs {
                    average_spectrum32: average_spectrum32.as_ref(),
                    stereo_spectrum64,
                    parallax_position: events
                        .pointer
                        .normalized_position_top_left()
                        .unwrap_or([0.5; 2]),
                    audio_material_values: &semantic_frame.audio_band_material_values,
                    material_scalar_values: &semantic_frame.material_scalar_values,
                },
            );
            frame.write_material_payload(scratch)?;
            true
        } else {
            false
        };
        let material_update_micros = elapsed_optional_micros(material_started);

        let scene_owned_started = cpu_timing_enabled.then(Instant::now);
        let scene_owned_uniform_updated = if self.scene_owned_uniform_plan.is_empty() {
            false
        } else {
            self.scene_owned_uniform_plan.write_payload(
                &graph.mesh_draws,
                SceneOwnedUniformFrameInputs {
                    scalar_overrides: &semantic_frame.material_scalar_values,
                    scene_time_seconds,
                    frame_delta_seconds,
                    audio_spectrum: events
                        .audio_spectrum()
                        .unwrap_or(&crate::engine::scene::StereoSpectrum64::ZERO),
                    parallax_position: events
                        .pointer
                        .normalized_position_top_left()
                        .unwrap_or([0.5; 2]),
                    sampled_binding_phase: reference_phase,
                },
                &mut self.scene_owned_uniform_scratch,
            )?;
            frame.write_scene_owned_uniform_payload(&self.scene_owned_uniform_scratch)?;
            true
        };
        let scene_owned_uniform_update_micros = elapsed_optional_micros(scene_owned_started);

        let skinning_started = cpu_timing_enabled.then(Instant::now);
        let skinning_storage_updated = if let Some(scratch) = self.skinning_scratch.as_mut() {
            pack_scene_skinning_palette_into(scratch, graph);
            frame.write_skinning_payload(scratch)?;
            true
        } else {
            false
        };
        let skinning_update_micros = elapsed_optional_micros(skinning_started);

        let draw_policy_started = cpu_timing_enabled.then(Instant::now);
        composite_scissor::update_scene_composite_scissors(
            storage,
            &self.mesh_coverage,
            graph,
            output_extent,
            &mut self.draw_commands,
        )?;
        self.scene_color_attachment_clear = scene_color_clear::resolve_scene_color_attachment_clear(
            storage,
            &self.mesh_coverage,
            graph,
            &self.scene_color_clear_graph_order,
            output_extent,
            self.scene_color_attachment_clear_enabled,
        );
        let draw_policy_update_micros = elapsed_optional_micros(draw_policy_started);

        Ok(SceneFrameBufferUpdate {
            transform_uniform_updated: true,
            material_uniform_updated,
            skinning_storage_updated,
            scene_owned_uniform_updated,
            dynamic_text_instance_updated,
            scene_color_attachment_clear: self.scene_color_attachment_clear,
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
}
