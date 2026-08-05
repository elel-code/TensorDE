//! Backend-neutral end-of-run report for the shared scene runtime.

use std::time::Duration;

use vulkan_renderer::{
    CompositeAlphaMode, Features, FullscreenSampledSurfaceTerminal, PresentMode,
    PresentationBootstrap,
};

use super::super::frame_events::SceneRuntimeEventSources;
use super::super::semantic_diagnostics::RenderingDeviceSceneDescriptorHeapSnapshot;
use super::super::{
    RenderingDeviceSceneDeviceSnapshot, RenderingDeviceScenePresentOptions,
    RenderingDeviceScenePresentSnapshot, RenderingDeviceSceneQueueSnapshot,
    RenderingDeviceSceneSwapchainSnapshot, scene_script_effect_visibility_snapshot,
    scene_semantic_diagnostics_snapshot,
};
use super::{SharedFrameStats, SharedSceneGpuResources};
use crate::engine::scene::semantic_world::SemanticFrameResolver;
use crate::renderer::wayland::WaylandSurfaceSnapshot;

pub(super) struct SharedSnapshotInputs<'a> {
    pub options: &'a RenderingDeviceScenePresentOptions,
    pub wayland_surface: WaylandSurfaceSnapshot,
    pub bootstrap: &'a PresentationBootstrap,
    pub terminal: &'a FullscreenSampledSurfaceTerminal,
    pub scene: &'a SharedSceneGpuResources,
    pub semantic_resolver: &'a SemanticFrameResolver,
    pub event_sources: &'a mut SceneRuntimeEventSources,
    pub stats: &'a SharedFrameStats,
    pub scene_color_4x_msaa: bool,
    pub elapsed: Duration,
    pub released_resource_payload_bytes: usize,
    pub released_texture_payload_bytes: usize,
    pub released_mesh_vertex_payload_bytes: usize,
    pub released_mesh_index_payload_bytes: usize,
    pub gpu_timing: Option<serde_json::Value>,
}

pub(super) fn build(
    inputs: SharedSnapshotInputs<'_>,
) -> Result<RenderingDeviceScenePresentSnapshot, String> {
    let SharedSnapshotInputs {
        options,
        wayland_surface,
        bootstrap,
        terminal,
        scene,
        semantic_resolver,
        event_sources,
        stats,
        scene_color_4x_msaa,
        elapsed,
        released_resource_payload_bytes,
        released_texture_payload_bytes,
        released_mesh_vertex_payload_bytes,
        released_mesh_index_payload_bytes,
        gpu_timing,
    } = inputs;
    let configuration = bootstrap.swapchain.configuration();
    let info = bootstrap.device.device_info();
    let features = bootstrap.device.features();
    let graphics_family = info.queues.graphics;
    let queue_count = info
        .queue_families
        .iter()
        .find(|family| family.index == graphics_family)
        .map_or(0, |family| family.queue_count);
    let selected_queue = RenderingDeviceSceneQueueSnapshot {
        physical_device_index: info.ordinal,
        physical_device_name: info.name.clone(),
        physical_device_type: format!("{:?}", info.device_type),
        queue_family_index: graphics_family,
        queue_count,
        supports_graphics: true,
        supports_present: true,
    };
    let device = RenderingDeviceSceneDeviceSnapshot {
        api_version: info.api_version.to_string(),
        roadmap_2026_ready: info.roadmap_2026_ready,
        available_device_extensions: info.extensions.iter().cloned().collect(),
        synchronization2_enabled: features.contains(Features::SYNCHRONIZATION2),
        dynamic_rendering_enabled: features.contains(Features::DYNAMIC_RENDERING),
        descriptor_heap_enabled: features.contains(Features::DESCRIPTOR_HEAP),
        pipeline_binaries_enabled: features.contains(Features::PIPELINE_BINARIES),
        fifo_latest_ready_enabled: features.contains(Features::FIFO_LATEST_READY),
        advanced_blend_enabled: features.contains(Features::ADVANCED_BLEND),
        advanced_blend_coherent: features.contains(Features::ADVANCED_BLEND_COHERENT),
        multisampled_render_to_single_sampled_enabled: features
            .contains(Features::MULTISAMPLED_RENDER_TO_SINGLE_SAMPLED),
    };
    let swapchain = RenderingDeviceSceneSwapchainSnapshot {
        created: true,
        format: format!("{:?}", configuration.format),
        color_space: format!("{:?}", configuration.color_space),
        present_mode: present_mode_label(configuration.present_mode),
        extent: (configuration.extent.width, configuration.extent.height),
        image_count: bootstrap.swapchain.image_count(),
        min_image_count: configuration.image_count,
        composite_alpha: composite_alpha_label(configuration.composite_alpha),
        image_usage: vec!["color-attachment"],
        present_id2_enabled: features.contains(Features::PRESENT_ID2),
        present_wait2_enabled: features.contains(Features::PRESENT_WAIT2),
    };

    let texture_memory_bytes = scene.cold.textures.allocation_bytes();
    let effect_target_memory_bytes = scene.cold.effect_targets.allocation_bytes();
    let transform_uniform_bytes = scene
        .frames
        .iter()
        .map(|frame| frame.transform.size())
        .sum();
    let material_uniform_bytes = scene
        .frames
        .iter()
        .filter_map(|frame| frame.material.as_ref())
        .map(vulkan_renderer::Buffer::size)
        .sum();
    let skinning_storage_bytes = scene
        .frames
        .iter()
        .filter_map(|frame| frame.skinning.as_ref())
        .map(vulkan_renderer::Buffer::size)
        .sum();
    let scene_owned_uniform_bytes = scene
        .frames
        .iter()
        .filter_map(|frame| frame.scene_owned_uniform.as_ref())
        .map(vulkan_renderer::Buffer::size)
        .sum();
    let scene_color_mesh_draw_count =
        super::super::draw_recording::draw_range_count(&scene.scene_color_draw_ranges);
    let scene_color_recorded_mesh_draw_count = scene
        .scene_color_draw_ranges
        .iter()
        .flat_map(|range| {
            let start = range.range.start as usize;
            let end = start.saturating_add(range.range.count as usize);
            scene.draw_commands.get(start..end).into_iter().flatten()
        })
        .filter(|draw| draw.enabled)
        .count();
    let descriptor_heap_resource_count = scene.resource_slot_kinds.len();
    let descriptor_heap_sampler_count = scene.sampler_descriptor_count;
    let semantic_diagnostics = options
        .semantic_diagnostics
        .then(|| {
            scene_semantic_diagnostics_snapshot(
                &options.storage,
                semantic_resolver,
                scene.frame_topology.graph(),
                &scene.draw_commands,
                &scene.descriptor_layout,
                &scene.sampled_binding_cycle,
                scene.material_scratch.as_deref(),
                &scene.scene_owned_uniform_plan,
                &scene.scene_owned_uniform_scratch,
                RenderingDeviceSceneDescriptorHeapSnapshot {
                    resource_descriptor_count: descriptor_heap_resource_count,
                    sampler_descriptor_count: descriptor_heap_sampler_count,
                    reference_phase_count: scene.sampled_binding_cycle.len(),
                    sampled_slots: scene.descriptor_layout.sampled_slots.clone(),
                    input_attachment_slots: scene.descriptor_layout.input_attachment_slots.clone(),
                },
            )
        })
        .transpose()?;
    let script_effect_visibility =
        scene_script_effect_visibility_snapshot(&options.storage, semantic_resolver)?;
    let _audio_summary = event_sources.audio_summary(stats.frames_presented != 0);
    let particle = scene.cold.particles.as_ref();
    let composite_scissor_draw_count = scene
        .draw_commands
        .iter()
        .filter(|draw| draw.scissor.is_some())
        .count();
    let composite_scissor_covered_pixels = scene
        .draw_commands
        .iter()
        .filter_map(|draw| draw.scissor)
        .map(|scissor| u64::from(scissor.extent[0]) * u64::from(scissor.extent[1]))
        .sum::<u64>();
    let full_pixels = u64::from(configuration.extent.width)
        .saturating_mul(u64::from(configuration.extent.height))
        .saturating_mul(composite_scissor_draw_count as u64);

    Ok(RenderingDeviceScenePresentSnapshot {
        binding: "vulkan-renderer",
        route: "shared-offscreen-scene-color-terminal-present",
        loader: "vulkan-renderer-owned".into(),
        requested_api_version: vulkan_renderer::ROADMAP_2026_API_VERSION.to_string(),
        runtime_elapsed_ms: elapsed.as_millis().min(u128::from(u64::MAX)) as u64,
        frames_presented: stats.frames_presented,
        average_present_fps: if elapsed.is_zero() {
            0.0
        } else {
            stats.frames_presented as f64 / elapsed.as_secs_f64()
        },
        present_delta_min_micros: stats.present_delta_min_micros,
        present_delta_max_micros: stats.present_delta_max_micros,
        present_delta_over_6250us_count: stats.present_delta_over_6250us_count,
        present_delta_over_8334us_count: stats.present_delta_over_8334us_count,
        clear_color: options.clear_color,
        wayland_surface,
        selected_queue,
        device,
        swapchain,
        command_submit_model: "slot-retirement -> record offscreen -> acquire -> terminal -> timeline submit -> present",
        uses_synchronization2: true,
        uses_submit2: true,
        uses_dynamic_rendering: true,
        scene_color_rasterization_samples: if scene_color_4x_msaa { "4x" } else { "1x" },
        uses_multisampled_render_to_single_sampled: bootstrap
            .device
            .features()
            .contains(Features::MULTISAMPLED_RENDER_TO_SINGLE_SAMPLED),
        uses_explicit_scene_color_msaa_resolve: false,
        scene_color_msaa_memory_bytes: 0,
        scene_color_target_model: "offscreen-live-physical-scene-color",
        scene_color_offscreen_image_count: terminal.target_count(),
        scene_color_offscreen_memory_bytes: terminal.target_allocation_size(),
        scene_color_offscreen_extent: (
            terminal.plan().target_extent.width,
            terminal.plan().target_extent.height,
        ),
        scene_color_distinct_from_swapchain: true,
        terminal_present_model: "slang-o2-terminal-present",
        frame_slot_count: scene.frames.len(),
        effect_target_physical_image_count: scene.cold.effect_targets.targets.len(),
        effect_target_memory_bytes,
        scene_color_mesh_draw_count,
        scene_color_recorded_mesh_draw_count,
        scene_color_attachment_clear_draw_count: usize::from(
            scene.scene_color_attachment_clear.is_some(),
        ),
        scene_color_attachment_clear_frame_count: stats.scene_color_attachment_clear_frame_count,
        descriptor_model: "VK_EXT_descriptor_heap",
        descriptor_heap_resource_count,
        descriptor_heap_sampler_count,
        vertex_buffer_bytes: scene.cold.mesh.vertex.size(),
        index_buffer_bytes: scene.cold.mesh.index.size(),
        transform_uniform_bytes,
        material_uniform_bytes,
        scene_owned_uniform_bytes,
        skinning_storage_bytes,
        scene_texture_image_count: scene.cold.textures.textures.len(),
        scene_texture_memory_bytes: texture_memory_bytes,
        released_resource_payload_bytes,
        released_texture_payload_bytes,
        released_mesh_vertex_payload_bytes,
        released_mesh_index_payload_bytes,
        transform_uniform_update_count: stats.transform_uniform_update_count,
        effect_uniform_update_count: stats.effect_uniform_update_count,
        skinning_storage_update_count: stats.skinning_storage_update_count,
        scene_owned_uniform_update_count: stats.scene_owned_uniform_update_count,
        frame_state_update_total_micros: stats.frame_state_update_total_micros,
        semantic_incremental_resolve_enabled: semantic_resolver.incremental_enabled(),
        semantic_retained_puppet_resolve_enabled: semantic_resolver.retained_puppet_enabled(),
        semantic_dynamic_entity_count: semantic_resolver.dynamic_entity_count(),
        scene_script_memory: semantic_resolver.script_memory_snapshot(),
        script_effect_visibility,
        semantic_diagnostics,
        semantic_resolve_total_micros: stats.semantic_resolve_total_micros,
        graph_update_total_micros: stats.graph_update_total_micros,
        transform_update_total_micros: stats.transform_update_total_micros,
        material_update_total_micros: stats.material_update_total_micros,
        skinning_update_total_micros: stats.skinning_update_total_micros,
        scene_owned_uniform_update_total_micros: stats.scene_owned_uniform_update_total_micros,
        draw_policy_update_total_micros: stats.draw_policy_update_total_micros,
        sampled_descriptor_update_count: 0,
        sampled_descriptor_update_total_micros: 0,
        command_recording_total_micros: stats.command_recording_total_micros,
        fence_wait_total_micros: 0,
        acquire_wait_total_micros: 0,
        queue_present_total_micros: 0,
        gpu_timing,
        composite_scissor_draw_count,
        composite_scissor_covered_pixels,
        composite_scissor_avoided_pixels: full_pixels
            .saturating_sub(composite_scissor_covered_pixels),
        scene_pipeline_count: scene.pipelines.entries.len(),
        scene_pipeline_machine_code_binary_count: scene.pipelines.machine_code_binary_count,
        scene_pipeline_machine_code_bytes: scene.pipelines.machine_code_bytes,
        scene_pipeline_machine_code_cache_hits: scene.pipelines.machine_code_cache_hits,
        scene_pipeline_machine_code_cache_misses: scene.pipelines.machine_code_cache_misses,
        mesh_draw_count: scene.draw_commands.len(),
        particle_gpu_emitter_count: particle.map_or(0, |particle| particle.emitter_count),
        particle_gpu_total_capacity: particle.map_or(0, |particle| particle.total_capacity),
        particle_gpu_state_bytes: particle.map_or(0, |particle| particle.state.size()),
        particle_gpu_indirect_bytes: particle.map_or(0, |particle| particle.indirect.size()),
        particle_gpu_frame_time_bytes: particle.map_or(0, |particle| particle.frame_time.size()),
        particle_gpu_simulation_bytes: particle.map_or(0, |particle| particle.simulation.size()),
        particle_gpu_device_local: true,
        particle_compute_pipeline_created: scene.pipelines.particle_compute.is_some(),
        particle_compute_dispatch_enabled: scene.pipelines.particle_compute.is_some(),
        mesh_draw_recorded: scene.draw_commands.iter().any(|draw| draw.enabled),
        command_order: shared_command_order(scene),
        present_backend: "vulkan-renderer-scene-present-runtime",
    })
}

fn shared_command_order(scene: &SharedSceneGpuResources) -> Vec<&'static str> {
    let plan = scene.effect_target_command_plan;
    super::super::command_order::scene_command_order(
        super::super::command_order::SceneCommandOrderFacts {
            no_sampled_slots: scene.sampler_descriptor_count == 0,
            input_attachment_slots_enabled: !scene.input_attachment_binding_cycle.is_empty(),
            fallback_texture_enabled: scene.cold.textures.white_fallback.is_some(),
            scene_textures_enabled: !scene.cold.textures.textures.is_empty(),
            skinning_buffer_enabled: scene.frames.iter().any(|frame| frame.skinning.is_some()),
            pipeline_variant_enabled: scene.pipelines.entries.len() > 1,
            dynamic_effect_uniforms_enabled: scene.dynamic_effect_uniforms,
            effect_targets_enabled: !scene.cold.effect_targets.targets.is_empty(),
            effect_target_copy_enabled: plan.copy_command_count != 0,
            effect_target_swap_enabled: plan.swap_reference_command_count != 0,
            effect_target_mesh_draw_enabled: plan.mesh_draw_count != 0,
            effect_target_fullscreen_draw_enabled: plan.fullscreen_triangle_draw_count != 0,
        },
    )
}

const fn present_mode_label(mode: PresentMode) -> &'static str {
    match mode {
        PresentMode::Immediate => "immediate",
        PresentMode::Mailbox => "mailbox",
        PresentMode::Fifo => "fifo",
        PresentMode::FifoRelaxed => "fifo-relaxed",
        PresentMode::FifoLatestReady => "fifo-latest-ready",
    }
}

const fn composite_alpha_label(mode: CompositeAlphaMode) -> &'static str {
    match mode {
        CompositeAlphaMode::Opaque => "opaque",
        CompositeAlphaMode::PreMultiplied => "pre-multiplied",
        CompositeAlphaMode::PostMultiplied => "post-multiplied",
        CompositeAlphaMode::Inherit => "inherit",
    }
}
