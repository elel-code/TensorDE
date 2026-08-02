//! Retained Gilder scene state owned entirely by `vulkan-renderer` objects.

mod execution_plan;
mod recording;

use vulkan_renderer::{
    Backend, DescriptorSlotKind, Extent2D, Features, MemoryAllocator, Queue, TextureFormat,
    UploadBelt,
};

use crate::engine::scene::SceneStorage;
use crate::renderer::native_vulkan::NativeVulkanSceneBackendPlan;

use super::composite_scissor::SceneMeshCoveragePlans;
use super::descriptor_layout::{ScenePipelineDescriptorLayout, scene_pipeline_descriptor_layout};
use super::descriptor_plan::scene_descriptor_plan_inputs;
use super::draw_parameter_layout;
use super::draw_recording::{SceneGpuDrawCommand, SceneGpuGraphDrawRange, scene_color_draw_ranges};
use super::draw_uniform::pack_scene_draw_uniforms;
use super::dynamic_text::SceneDynamicTextRuntime;
use super::effect_target::{
    SceneEffectTargetCommand, SceneEffectTargetCommandPlan, SceneEffectTargetImagePlan,
    SharedSceneEffectExecutionPlan, apply_scene_effect_target_input_attachment_usage,
    apply_scene_effect_target_local_read_candidate_usage,
    apply_scene_effect_target_local_read_scope_usage, scene_effect_target_command_plan,
    scene_effect_target_commands, scene_effect_target_image_plan,
    shared_scene_effect_execution_plans,
};
use super::frame_state::video::pack_scene_video_vertices_into;
use super::frame_state::{SceneFrameTopology, pack_scene_skinning_palette};
use super::graph_execution::{scene_graph_draw_execution_order, scene_graph_execution_order};
use super::input_attachment_binding::{
    SceneInputAttachmentBindingPlan, scene_input_attachment_binding_cycle,
};
use super::local_read::{
    SceneLocalReadScopePassRole, SceneLocalReadScopePlan, scene_local_read_scope_plans,
};
use super::material_uniform::pack_scene_material_uniforms;
use super::mesh_payload::{pack_scene_indices, pack_scene_vertices};
use super::native_descriptor_push::resolve_scene_shared_descriptor_pushes;
use super::pipeline::{
    ScenePipelineResourceCreateInputs, ScenePipelineResources, create_scene_pipelines,
    scene_disabled_pipeline_indices_for_draws_with_local_read,
    scene_pipeline_indices_for_draws_with_local_read,
};
use super::sampled_binding::video::apply_scene_video_draw_semantics;
use super::sampled_binding::{SceneSampledImageBindingPlan, scene_sampled_image_binding_cycle};
use super::scene_owned_uniform::{SceneOwnedUniformArenaPlan, SceneOwnedUniformFrameInputs};
use super::shared_resources::{
    SharedSceneColdResourceInputs, SharedSceneColdResources, SharedSceneDescriptorInputs,
    SharedSceneFramePayloads, SharedSceneFrameResources,
};
use execution_plan::{SharedSceneFrameExecutionPlan, compile_shared_scene_frame_execution_plan};

pub(super) struct SharedSceneGpuResources {
    pub cold: SharedSceneColdResources,
    pub frames: Vec<SharedSceneFrameResources>,
    pub pipelines: ScenePipelineResources,
    pub draw_commands: Vec<SceneGpuDrawCommand>,
    pub descriptor_layout: ScenePipelineDescriptorLayout,
    pub sampled_binding_cycle: Vec<SceneSampledImageBindingPlan>,
    pub input_attachment_binding_cycle: Vec<SceneInputAttachmentBindingPlan>,
    pub effect_target_plans: Vec<SceneEffectTargetImagePlan>,
    pub effect_target_commands: Vec<SceneEffectTargetCommand>,
    pub effect_target_command_plan: SceneEffectTargetCommandPlan,
    pub effect_execution_plans: Vec<SharedSceneEffectExecutionPlan>,
    frame_execution_plan: SharedSceneFrameExecutionPlan,
    pub local_read_scopes: Vec<SceneLocalReadScopePlan>,
    pub local_read_mappings: Vec<SharedSceneLocalReadMappings>,
    pub scene_color_draw_ranges: Vec<SceneGpuGraphDrawRange>,
    pub graph_execution_order: Vec<u32>,
    pub(super) scene_color_clear_graph_order: Vec<u32>,
    pub frame_topology: SceneFrameTopology,
    pub scene_owned_uniform_plan: SceneOwnedUniformArenaPlan,
    pub transform_scratch: Vec<u8>,
    pub video_vertex_scratch: Vec<u8>,
    pub scene_owned_uniform_scratch: Vec<u8>,
    pub material_scratch: Option<Vec<u8>>,
    pub skinning_scratch: Option<Vec<u8>>,
    pub dynamic_text: SceneDynamicTextRuntime,
    pub dynamic_effect_uniforms: bool,
    pub resource_slot_kinds: Vec<DescriptorSlotKind>,
    pub sampler_descriptor_count: usize,
    pub mesh_coverage: SceneMeshCoveragePlans,
    pub scene_color_attachment_clear_enabled: bool,
    pub scene_color_attachment_clear: Option<super::scene_color_clear::SceneGpuSceneColorClear>,
    pub graph: crate::engine::scene::SceneRenderingDeviceGraphPlan,
}

pub(super) struct SharedSceneLocalReadMappings {
    pub producer: vulkan_renderer::RenderingLocalReadMapping,
    pub consumer: vulkan_renderer::RenderingLocalReadMapping,
}

impl SharedSceneGpuResources {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn create(
        device: &Backend,
        allocator: &MemoryAllocator,
        upload_belt: &mut UploadBelt,
        queue: &Queue,
        storage: &SceneStorage,
        backend_plan: NativeVulkanSceneBackendPlan,
        target_format: TextureFormat,
        extent: Extent2D,
        scene_color_views: &[vulkan_renderer::ImageView],
        frame_slot_count: usize,
        scene_color_msaa_enabled: bool,
        multisampled_render_to_single_sampled_enabled: bool,
        pipeline_binary_cache: &vulkan_renderer::PipelineBinaryArchiveCache,
    ) -> Result<Self, String> {
        if extent.is_empty() || frame_slot_count == 0 || scene_color_views.len() != frame_slot_count
        {
            return Err(
                "shared scene requires non-empty extent and one SceneColor view per frame slot"
                    .into(),
            );
        }
        let graph = backend_plan.rendering_device_graph;
        if graph.mesh_draws.is_empty() {
            return Err("scene present requires at least one render graph mesh draw".into());
        }
        if scene_color_msaa_enabled && !multisampled_render_to_single_sampled_enabled {
            return Err(
                "shared SceneColor 4x MSAA requires retained explicit resolve targets before the ownership flip"
                    .into(),
            );
        }

        let descriptor_layout = scene_pipeline_descriptor_layout(storage, &graph)?;
        let sampled_binding_cycle = scene_sampled_image_binding_cycle(
            &graph,
            &descriptor_layout.sampled_slots,
            &descriptor_layout.input_attachment_slots,
        )?;
        if sampled_binding_cycle.is_empty() {
            return Err("scene sampled binding cycle is empty".to_owned());
        }
        let input_attachment_binding_cycle = scene_input_attachment_binding_cycle(
            storage,
            &graph,
            &descriptor_layout.input_attachment_slots,
            &sampled_binding_cycle,
        )?;
        if input_attachment_binding_cycle.len() != sampled_binding_cycle.len() {
            return Err("scene sampled/input-attachment phase cycles differ".to_owned());
        }
        let mut effect_target_plans =
            scene_effect_target_image_plan(storage, &graph, target_format, extent)?;
        apply_scene_effect_target_input_attachment_usage(
            &mut effect_target_plans,
            &input_attachment_binding_cycle,
        )?;
        apply_scene_effect_target_local_read_candidate_usage(
            &mut effect_target_plans,
            &graph,
            &sampled_binding_cycle,
        )?;
        let local_read_scopes =
            scene_local_read_scope_plans(storage, &graph, &effect_target_plans)?;
        apply_scene_effect_target_local_read_scope_usage(
            &mut effect_target_plans,
            &graph,
            &local_read_scopes,
            &sampled_binding_cycle,
        )?;
        let local_read_mappings = local_read_scopes
            .iter()
            .map(|scope| {
                Ok(SharedSceneLocalReadMappings {
                    producer: scope
                        .shared_mapping(device, SceneLocalReadScopePassRole::Producer)?,
                    consumer: scope
                        .shared_mapping(device, SceneLocalReadScopePassRole::Consumer)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        let properties = device.device_info().properties;
        let limits = device.device_info().limits;
        let scene_owned_uniform_plan = SceneOwnedUniformArenaPlan::build(
            storage,
            &graph.mesh_draws,
            &descriptor_layout,
            &sampled_binding_cycle,
            &effect_target_plans,
            [extent.width, extent.height],
            properties.min_uniform_buffer_offset_alignment,
        )?;
        let mut scene_owned_uniform_scratch =
            vec![
                0;
                usize::try_from(scene_owned_uniform_plan.byte_count).map_err(|_| {
                    "scene-owned uniform arena exceeds host address space".to_owned()
                })?
            ];
        if !scene_owned_uniform_plan.is_empty() {
            scene_owned_uniform_plan.write_payload(
                &graph.mesh_draws,
                SceneOwnedUniformFrameInputs::INITIAL,
                &mut scene_owned_uniform_scratch,
            )?;
        }

        let pipeline_indices = scene_pipeline_indices_for_draws_with_local_read(
            storage,
            &graph,
            target_format,
            &effect_target_plans,
            &local_read_scopes,
            scene_color_msaa_enabled,
        )?;
        let disabled_pipeline_indices = scene_disabled_pipeline_indices_for_draws_with_local_read(
            storage,
            &graph,
            target_format,
            &effect_target_plans,
            &local_read_scopes,
            scene_color_msaa_enabled,
        )?;
        let vertex_payload = pack_scene_vertices(storage, &graph)?;
        let index_payload = pack_scene_indices(storage, &graph);
        let dynamic_text = SceneDynamicTextRuntime::from_storage(storage)?;
        let mut transform_scratch = pack_scene_draw_uniforms(
            storage,
            &graph.mesh_draws,
            0.0,
            [extent.width, extent.height],
        );
        transform_scratch.resize(
            transform_scratch
                .len()
                .saturating_add(dynamic_text.byte_capacity()),
            0,
        );
        let material_payload = descriptor_layout.material_uniform_enabled.then(|| {
            pack_scene_material_uniforms(
                storage,
                &graph.mesh_draws,
                0.0,
                [extent.width, extent.height],
            )
        });
        let skinning_payload = descriptor_layout
            .skinning_storage_enabled
            .then(|| pack_scene_skinning_palette(&graph));

        let (resource_descriptors, mut draw_commands) = scene_descriptor_plan_inputs(
            storage,
            &graph.mesh_draws,
            &graph.particle_gpu_emitters,
            &descriptor_layout,
            &pipeline_indices,
            &disabled_pipeline_indices,
        );
        apply_scene_video_draw_semantics(&graph, &mut draw_commands)?;
        let mut video_vertex_scratch = Vec::new();
        pack_scene_video_vertices_into(
            &mut video_vertex_scratch,
            storage,
            &graph,
            &draw_commands,
            [extent.width, extent.height],
        )?;
        let mut resource_slot_kinds = resource_descriptors;
        let particle_global_descriptor_base =
            (!graph.particle_gpu_emitters.is_empty()).then(|| {
                let base = resource_slot_kinds.len();
                resource_slot_kinds.extend([DescriptorSlotKind::StorageBuffer; 3]);
                base
            });
        let sampler_descriptor_count = descriptor_layout
            .sampler_count_per_draw()
            .saturating_mul(graph.mesh_draws.len());
        resolve_scene_shared_descriptor_pushes(
            storage,
            &graph.mesh_draws,
            &descriptor_layout,
            &resource_slot_kinds,
            sampler_descriptor_count,
            limits.descriptor_heap.max_push_data_size,
            &mut draw_commands,
        )?;

        let cold = SharedSceneColdResources::create(
            allocator,
            upload_belt,
            queue,
            SharedSceneColdResourceInputs {
                vertex_payload: &vertex_payload,
                index_payload: &index_payload,
                storage,
                graph: &graph,
                sampled_binding_cycle: &sampled_binding_cycle,
                effect_targets: &effect_target_plans,
            },
        )?;
        let mut frames = Vec::with_capacity(frame_slot_count);
        for (frame_slot, scene_color_view) in scene_color_views.iter().enumerate() {
            let mut frame = SharedSceneFrameResources::create(
                allocator,
                frame_slot,
                SharedSceneFramePayloads {
                    transform: &transform_scratch,
                    video_vertex: (!video_vertex_scratch.is_empty())
                        .then_some(video_vertex_scratch.as_slice()),
                    material: material_payload.as_deref(),
                    skinning: skinning_payload.as_deref(),
                    scene_owned_uniform: (!scene_owned_uniform_plan.is_empty())
                        .then_some(scene_owned_uniform_scratch.as_slice()),
                },
                resource_slot_kinds.len() as u64,
                sampler_descriptor_count as u64,
            )?;
            for (reference_phase, (sampled_binding_plan, input_attachment_binding_plan)) in
                sampled_binding_cycle
                    .iter()
                    .zip(&input_attachment_binding_cycle)
                    .enumerate()
            {
                frame.lower_descriptor_phase(
                    device,
                    frame_slot,
                    reference_phase,
                    SharedSceneDescriptorInputs {
                        slot_kinds: &resource_slot_kinds,
                        draw_commands: &draw_commands,
                        scene_owned_uniform_plan: &scene_owned_uniform_plan,
                        sampled_binding_plan,
                        input_attachment_binding_plan,
                        cold: &cold,
                        scene_color: Some(scene_color_view),
                        particle_global_descriptor_base,
                    },
                )?;
            }
            frames.push(frame);
        }

        let enabled_features = device.features();
        let pipelines = create_scene_pipelines(ScenePipelineResourceCreateInputs {
            device,
            target_format,
            extent,
            storage,
            graph: &graph,
            resource_descriptor_kinds: &resource_slot_kinds,
            particle_global_descriptor_base,
            effect_target_plans: &effect_target_plans,
            advanced_blend_enabled: enabled_features.contains(Features::ADVANCED_BLEND),
            advanced_blend_coherent: enabled_features.contains(Features::ADVANCED_BLEND_COHERENT),
            scene_color_msaa_enabled,
            local_read_scopes: &local_read_scopes,
            pipeline_binary_cache,
        })?;
        let effect_target_commands = scene_effect_target_commands(storage, &graph);
        let effect_target_command_plan =
            scene_effect_target_command_plan(&effect_target_commands, &graph);
        let effect_execution_plans = shared_scene_effect_execution_plans(
            &effect_target_commands,
            &graph.target_allocations,
            &effect_target_plans,
            &sampled_binding_cycle,
            &local_read_scopes,
        )?;
        let graph_execution_order = scene_graph_execution_order(&graph);
        let scene_color_clear_graph_order = scene_graph_draw_execution_order(&graph);
        let frame_execution_plan = compile_shared_scene_frame_execution_plan(
            &graph,
            &graph_execution_order,
            &effect_target_plans,
            &effect_execution_plans,
        )?;
        let dynamic_effect_uniforms = storage.script_programs().iter().any(|program| {
            program.target == crate::engine::scene::SceneScriptTarget::MaterialScalar
        }) || graph.mesh_draws.iter().any(|draw| {
            draw_parameter_layout(storage, draw).uses_dynamic_material_input()
                || matches!(
                    draw.effect_visibility_policy,
                    crate::engine::scene::SceneRenderEffectVisibilityPolicy::WaterWavesStages
                        | crate::engine::scene::SceneRenderEffectVisibilityPolicy::FlatRoundedMask
                        | crate::engine::scene::SceneRenderEffectVisibilityPolicy::MaterialStages
                )
        });

        Ok(Self {
            cold,
            frames,
            pipelines,
            draw_commands,
            descriptor_layout,
            sampled_binding_cycle,
            input_attachment_binding_cycle,
            effect_target_plans,
            effect_target_command_plan,
            effect_execution_plans,
            frame_execution_plan,
            effect_target_commands,
            local_read_scopes,
            local_read_mappings,
            scene_color_draw_ranges: scene_color_draw_ranges(&graph),
            graph_execution_order,
            scene_color_clear_graph_order,
            frame_topology: SceneFrameTopology::from_owned_graph(graph.clone()),
            scene_owned_uniform_plan,
            transform_scratch,
            video_vertex_scratch,
            scene_owned_uniform_scratch,
            material_scratch: material_payload,
            skinning_scratch: skinning_payload,
            dynamic_text,
            dynamic_effect_uniforms,
            resource_slot_kinds,
            sampler_descriptor_count,
            mesh_coverage: SceneMeshCoveragePlans::from_storage(storage),
            scene_color_attachment_clear_enabled: std::env::var_os(
                "GILDER_NATIVE_VULKAN_DISABLE_SCENE_COLOR_ATTACHMENT_CLEAR",
            )
            .is_none(),
            scene_color_attachment_clear: None,
            graph,
        })
    }
}
