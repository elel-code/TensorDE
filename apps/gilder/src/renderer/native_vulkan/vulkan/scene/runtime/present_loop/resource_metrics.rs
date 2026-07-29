//! End-of-run retained scene resource metrics used by the present report.

use super::super::*;
use crate::engine::scene::SceneRenderingDeviceDrawPrimitive;

pub(super) struct ScenePresentResourceMetrics {
    pub(super) vertex_buffer_bytes: u64,
    pub(super) index_buffer_bytes: u64,
    pub(super) transform_uniform_bytes: u64,
    pub(super) material_uniform_bytes: u64,
    pub(super) scene_owned_uniform_bytes: u64,
    pub(super) skinning_storage_bytes: u64,
    pub(super) resource_residency: NativeVulkanSceneResourceResidencySnapshot,
    pub(super) sampled_fallback_texture_count: usize,
    pub(super) sampled_fallback_descriptor_count: usize,
    pub(super) sampled_scene_texture_descriptor_count: usize,
    pub(super) sampled_scene_color_snapshot_descriptor_count: usize,
    pub(super) sampled_effect_target_descriptor_count: usize,
    pub(super) effect_target_reference_cycle_length: usize,
    pub(super) descriptor_heap_resource_count: usize,
    pub(super) descriptor_heap_sampler_count: usize,
    pub(super) scene_texture_image_count: usize,
    pub(super) scene_texture_memory_bytes: u64,
    pub(super) effect_target_physical_image_count: usize,
    pub(super) effect_target_memory_bytes: u64,
    pub(super) effect_target_dynamic_rendering_recorded: bool,
    pub(super) effect_target_dynamic_rendering_pass_count: usize,
    pub(super) effect_batch_count: usize,
    pub(super) effect_batch_instance_count: usize,
    pub(super) effect_batch_field_count: usize,
    pub(super) effect_target_copy_command_count: usize,
    pub(super) effect_target_swap_reference_count: usize,
    pub(super) effect_target_mesh_draw_count: usize,
    pub(super) effect_target_discard_load_count: usize,
    pub(super) effect_target_fullscreen_draw_count: usize,
    pub(super) scene_color_mesh_draw_count: usize,
    pub(super) scene_color_attachment_clear_draw_count: usize,
    pub(super) scene_color_recorded_mesh_draw_count: usize,
    pub(super) scene_pipeline_count: usize,
    pub(super) scene_color_msaa_enabled: bool,
    pub(super) multisampled_render_to_single_sampled_enabled: bool,
    pub(super) scene_color_msaa_memory_bytes: u64,
    pub(super) mesh_draw_count: usize,
    pub(super) particle_compute_pipeline_created: bool,
    pub(super) particle_instance_capacity: u64,
    pub(super) particle_gpu_emitter_count: u32,
    pub(super) particle_gpu_total_capacity: u64,
    pub(super) particle_gpu_state_bytes: u64,
    pub(super) particle_gpu_indirect_bytes: u64,
    pub(super) particle_gpu_frame_time_bytes: u64,
    pub(super) particle_gpu_device_local: bool,
}

pub(super) fn scene_present_resource_metrics(
    scene_resources: &SceneGpuResources,
) -> ScenePresentResourceMetrics {
let vertex_buffer_bytes = scene_resources
    .mesh_uploads
    .vertex
    .target
    .snapshot
    .requested_bytes;
let index_buffer_bytes = scene_resources
    .mesh_uploads
    .index
    .target
    .snapshot
    .requested_bytes;
let transform_uniform_bytes = scene_resources
    .frame_resources
    .iter()
    .map(|frame| frame.transform_buffer.snapshot.requested_bytes)
    .sum();
let material_uniform_bytes = scene_resources
    .frame_resources
    .iter()
    .filter_map(|frame| frame.material_buffer.as_ref())
    .map(|buffer| buffer.snapshot.requested_bytes)
    .sum();
let scene_owned_uniform_bytes = scene_resources
    .frame_resources
    .iter()
    .filter_map(|frame| frame.scene_owned_uniform_buffer.as_ref())
    .map(|buffer| buffer.snapshot.requested_bytes)
    .sum();
let skinning_storage_bytes = scene_resources
    .frame_resources
    .iter()
    .filter_map(|frame| frame.skinning_buffer.as_ref())
    .map(|buffer| buffer.snapshot.requested_bytes)
    .sum();
let resource_residency =
    resource_residency::scene_resource_residency_snapshot(scene_resources);
let sampled_fallback_texture_count = usize::from(scene_resources.white_upload.is_some());
let sampled_fallback_descriptor_count = scene_resources
    .sampled_binding_cycle
    .first()
    .map_or(0, |plan| plan.fallback_descriptor_count);
let sampled_scene_texture_descriptor_count = scene_resources
    .sampled_binding_cycle
    .first()
    .map_or(0, |plan| plan.scene_texture_descriptor_count);
let sampled_scene_color_snapshot_descriptor_count = scene_resources
    .sampled_binding_cycle
    .first()
    .map_or(0, |plan| plan.scene_color_snapshot_descriptor_count);
let sampled_effect_target_descriptor_count = scene_resources
    .sampled_binding_cycle
    .first()
    .map_or(0, |plan| plan.effect_target_descriptor_count);
let effect_target_reference_cycle_length = scene_resources.sampled_binding_cycle.len();
let descriptor_heap_resource_count = scene_resources
    .descriptor_heap_plan
    .resource_descriptor_count;
let descriptor_heap_sampler_count = scene_resources.descriptor_heap_plan.sampler_count;
let scene_texture_image_count = scene_resources.scene_textures.len();
let scene_texture_memory_bytes =
    scene_texture::scene_texture_memory_bytes(&scene_resources.scene_textures);
let effect_target_physical_image_count = scene_resources.effect_targets.len();
let effect_target_memory_bytes =
    effect_target::effect_target_memory_bytes(&scene_resources.effect_targets);
let effect_target_dynamic_rendering_recorded = effect_target_physical_image_count > 0;
let effect_target_dynamic_rendering_pass_count = scene_resources
    .effect_target_command_plan
    .dynamic_rendering_pass_count;
let effect_batch_count = scene_resources
    .effect_targets
    .iter()
    .filter(|target| target.plan.batch_field_count > 1)
    .count();
let effect_batch_instance_count =
    effect_target::effect_batch_instance_count(&scene_resources.effect_target_commands);
let effect_batch_field_count = scene_resources
    .effect_targets
    .iter()
    .filter(|target| target.plan.batch_field_count > 1)
    .map(|target| target.plan.batch_field_count as usize)
    .sum();
let effect_target_copy_command_count = scene_resources
    .effect_target_command_plan
    .copy_command_count;
let effect_target_swap_reference_count = scene_resources
    .effect_target_command_plan
    .swap_reference_command_count;
let effect_target_mesh_draw_count = scene_resources.effect_target_command_plan.mesh_draw_count;
let effect_target_discard_load_count = scene_resources
    .effect_target_command_plan
    .discard_load_count;
let effect_target_fullscreen_draw_count = scene_resources
    .effect_target_command_plan
    .fullscreen_triangle_draw_count;
let scene_color_mesh_draw_count = draw_range_count(&scene_resources.scene_color_draw_ranges);
let scene_color_attachment_clear_draw_count =
    usize::from(scene_resources.scene_color_attachment_clear.is_some());
let scene_color_recorded_mesh_draw_count =
    scene_color_mesh_draw_count.saturating_sub(scene_color_attachment_clear_draw_count);
let scene_pipeline_count = scene_resources.pipelines.entries.len();
let scene_color_msaa_enabled = scene_resources.scene_color_msaa_enabled;
let multisampled_render_to_single_sampled_enabled =
    scene_resources.multisampled_render_to_single_sampled_enabled;
let scene_color_msaa_memory_bytes =
    scene_color_msaa::scene_color_msaa_memory_bytes(&scene_resources.scene_color_msaa_targets);
let mesh_draw_count = scene_resources.draw_commands.len();
let particle_compute_pipeline_created = scene_resources.pipelines.particle_compute.is_some();
let particle_instance_capacity = scene_resources
    .draw_commands
    .iter()
    .filter(|draw| draw.primitive == SceneRenderingDeviceDrawPrimitive::ParticleBillboard)
    .map(|draw| u64::from(draw.instance_capacity))
    .sum();
let (
    particle_gpu_emitter_count,
    particle_gpu_total_capacity,
    particle_gpu_state_bytes,
    particle_gpu_indirect_bytes,
    particle_gpu_frame_time_bytes,
    particle_gpu_device_local,
) = scene_resources
    .particle_resources
    .as_ref()
    .map_or((0, 0, 0, 0, 0, false), |resources| {
        let state = &resources.state_upload.target.snapshot;
        let indirect = &resources.indirect_upload.target.snapshot;
        let frame_time = &resources.frame_time.target.snapshot;
        (
            resources.emitter_count,
            resources.total_capacity,
            state.requested_bytes,
            indirect.requested_bytes,
            frame_time.requested_bytes,
            state
                .selected_memory_property_flags
                .contains(&"device-local")
                && indirect
                    .selected_memory_property_flags
                    .contains(&"device-local")
                && frame_time
                    .selected_memory_property_flags
                    .contains(&"device-local"),
        )
    });
    ScenePresentResourceMetrics {
        vertex_buffer_bytes,
        index_buffer_bytes,
        transform_uniform_bytes,
        material_uniform_bytes,
        scene_owned_uniform_bytes,
        skinning_storage_bytes,
        resource_residency,
        sampled_fallback_texture_count,
        sampled_fallback_descriptor_count,
        sampled_scene_texture_descriptor_count,
        sampled_scene_color_snapshot_descriptor_count,
        sampled_effect_target_descriptor_count,
        effect_target_reference_cycle_length,
        descriptor_heap_resource_count,
        descriptor_heap_sampler_count,
        scene_texture_image_count,
        scene_texture_memory_bytes,
        effect_target_physical_image_count,
        effect_target_memory_bytes,
        effect_target_dynamic_rendering_recorded,
        effect_target_dynamic_rendering_pass_count,
        effect_batch_count,
        effect_batch_instance_count,
        effect_batch_field_count,
        effect_target_copy_command_count,
        effect_target_swap_reference_count,
        effect_target_mesh_draw_count,
        effect_target_discard_load_count,
        effect_target_fullscreen_draw_count,
        scene_color_mesh_draw_count,
        scene_color_attachment_clear_draw_count,
        scene_color_recorded_mesh_draw_count,
        scene_pipeline_count,
        scene_color_msaa_enabled,
        multisampled_render_to_single_sampled_enabled,
        scene_color_msaa_memory_bytes,
        mesh_draw_count,
        particle_compute_pipeline_created,
        particle_instance_capacity,
        particle_gpu_emitter_count,
        particle_gpu_total_capacity,
        particle_gpu_state_bytes,
        particle_gpu_indirect_bytes,
        particle_gpu_frame_time_bytes,
        particle_gpu_device_local,
    }
}
