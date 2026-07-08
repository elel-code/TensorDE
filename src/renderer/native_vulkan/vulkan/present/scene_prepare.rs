//! Scene cold prepare stage for Vulkanalia scene present.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/effect-semantics.md`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SceneEffectPassGraphPlan, SceneFramePlan, SceneGraphExecutionPlan, SceneGraphTarget,
    SceneObject, SceneResource,
};
use crate::renderer::native_vulkan::scene_backend::effect_pipeline_prepare::{
    NativeVulkanSceneEffectPipelinePreparePlan,
    native_vulkan_prepare_scene_effect_pipeline_cache_with_target_formats,
};
use crate::renderer::native_vulkan::scene_backend::effect_targets::NativeVulkanSceneEffectTargetPlan;
use crate::renderer::native_vulkan::scene_backend::frame_command_buffer::{
    native_vulkan_begin_scene_frame_command_buffer, native_vulkan_end_scene_frame_command_buffer,
};
use crate::renderer::native_vulkan::scene_backend::frame_resources::NativeVulkanSceneFrameResources;
use crate::renderer::native_vulkan::scene_backend::frame_slots::NativeVulkanSceneFrameSlotResources;
use crate::renderer::native_vulkan::scene_backend::frame_submit::{
    NativeVulkanScenePrepareSubmitContext, NativeVulkanScenePrepareSubmitPlan,
    native_vulkan_submit_scene_prepare_commands2,
};
use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_executor::NativeVulkanSceneLayerAlphaMaskDescriptorPlan;
use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_executor::{
    native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_draws,
    native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_pipelines_from_targets,
    native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_targets,
    native_vulkan_plan_scene_layer_alpha_mask_resource_binds,
    native_vulkan_plan_scene_layer_alpha_mask_runtime_frame,
    native_vulkan_plan_scene_layer_alpha_mask_token_schedule,
};
use crate::renderer::native_vulkan::scene_backend::pipeline_prepare::{
    NativeVulkanSceneMeshPipelinePreparePlan,
    native_vulkan_prepare_scene_mesh_pipeline_cache_with_shader_catalog_and_extra_keys,
};
use crate::renderer::native_vulkan::scene_backend::resource_prepare::{
    NativeVulkanSceneMeshResourcePrepareContext, NativeVulkanSceneMeshResourcePreparePlan,
    native_vulkan_record_scene_mesh_resource_prepare_frame,
};
use crate::renderer::native_vulkan::scene_backend::runtime::native_vulkan_resolve_scene_layer_490_color_target;
use crate::renderer::native_vulkan::scene_backend::shader_artifacts::{
    NativeVulkanSceneEffectShaderArtifactCatalog, NativeVulkanSceneShaderArtifactCatalog,
};
use crate::renderer::native_vulkan::scene_backend::target_formats::NativeVulkanSceneGraphTargetFormatPlan;
use crate::renderer::native_vulkan::vulkan::NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanVulkanaliaScenePrepareSnapshot {
    pub resource_prepare: NativeVulkanVulkanaliaSceneResourcePrepareSnapshot,
    pub pipeline_prepare: NativeVulkanVulkanaliaScenePipelinePrepareSnapshot,
    pub effect_pipeline_prepare: NativeVulkanVulkanaliaSceneEffectPipelinePrepareSnapshot,
    pub prepare_submit: NativeVulkanVulkanaliaScenePrepareSubmitSnapshot,
    pub scene_shader_count: usize,
    pub graph_target_format_count: usize,
    pub effect_target_count: usize,
    pub effect_texture_descriptor_binding_count: usize,
    pub effect_uniform_gpu_buffer_action_count: usize,
    pub effect_resource_heap_action_count: usize,
    pub effect_heap_slice_count: usize,
    pub effect_resource_descriptor_count: usize,
    pub effect_sampler_descriptor_count: usize,
    pub layer_alpha_mask_heap_bind_count: usize,
    pub layer_alpha_mask_resource_heap_action_count: usize,
    pub layer_alpha_mask_heap_slice_count: usize,
    pub layer_alpha_mask_resource_descriptor_count: usize,
    pub layer_alpha_mask_sampler_descriptor_count: usize,
    pub offscreen_target_count: usize,
    pub offscreen_target_action_count: usize,
    pub cold_prepare_wait: &'static str,
    pub command_order: [&'static str; 13],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaScenePipelinePrepareSnapshot {
    pub target_format: String,
    pub target_formats: Vec<String>,
    pub target_format_count: usize,
    pub draw_count: usize,
    pub cache_key_count: usize,
    pub created_pipeline_count: usize,
    pub reused_pipeline_count: usize,
    pub resource_descriptor_count: usize,
    pub sampler_descriptor_count: usize,
    pub descriptor_model: &'static str,
    pub command_order: [&'static str; 5],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSceneEffectPipelinePrepareSnapshot {
    pub target_formats: Vec<String>,
    pub target_format_count: usize,
    pub material_pass_count: usize,
    pub cache_key_count: usize,
    pub shader_artifact_count: usize,
    pub created_pipeline_count: usize,
    pub reused_pipeline_count: usize,
    pub resource_descriptor_count: usize,
    pub sampler_descriptor_count: usize,
    pub descriptor_model: &'static str,
    pub command_order: [&'static str; 6],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaScenePrepareSubmitSnapshot {
    pub frame_slot: u32,
    pub submission_index: u64,
    pub wait_stage: &'static str,
    pub signal_stage: &'static str,
    pub command_order: [&'static str; 2],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSceneResourcePrepareSnapshot {
    pub residency_command_count: usize,
    pub material_uniform_gpu_buffer_action_count: usize,
    pub texture_descriptor_binding_count: usize,
    pub resource_heap_action_count: usize,
    pub texture_image_action_count: usize,
    pub gpu_buffer_action_count: usize,
    pub descriptor_model: &'static str,
    pub resource_descriptor_count: usize,
    pub sampler_descriptor_count: usize,
    pub command_order: [&'static str; 7],
}

#[allow(clippy::too_many_arguments)]
pub(in crate::renderer::native_vulkan::vulkan) fn prepare_scene_resources_and_pipelines(
    device: &Device,
    queue: vk::Queue,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    frame_slots: &mut NativeVulkanSceneFrameSlotResources,
    frame_resources: &mut NativeVulkanSceneFrameResources,
    resources: &[SceneResource],
    objects: &[SceneObject],
    frame: &SceneFramePlan,
    graph_execution: &SceneGraphExecutionPlan,
    post_effect_graph: &SceneEffectPassGraphPlan,
    target_formats: &NativeVulkanSceneGraphTargetFormatPlan,
    swapchain_extent: vk::Extent2D,
    shader_catalog: &NativeVulkanSceneShaderArtifactCatalog,
    effect_shader_catalog: &NativeVulkanSceneEffectShaderArtifactCatalog,
) -> Result<NativeVulkanVulkanaliaScenePrepareSnapshot, String> {
    let prepare_slot = 0u32;
    let slot_prepare = frame_slots
        .try_prepare_frame_slot(device, prepare_slot)?
        .ok_or_else(|| "scene cold prepare slot is still in flight".to_owned())?;
    if let Some(completed) = slot_prepare.completed_submission {
        let _ = frame_resources.release_completed_frame_resources(device, completed);
    }

    let frame_submission = frame_slots.begin_frame_submission(prepare_slot)?;
    let slot_sync = frame_slots.slot_sync(prepare_slot)?;
    let mut submitted_to_queue = false;
    let result = (|| -> Result<NativeVulkanVulkanaliaScenePrepareSnapshot, String> {
        let effect_target_plan =
            NativeVulkanSceneEffectTargetPlan::from_effect_pass_graph_with_layer_compositor(
                post_effect_graph,
                Some(&frame.layer_compositor),
                swapchain_extent,
                target_formats.format(SceneGraphTarget::Swapchain)?,
            )?;
        let offscreen_target_plan = frame_resources
            .offscreen_target_frame_plan_with_effect_targets(
                graph_execution,
                swapchain_extent,
                |target| target_formats.format(target),
                effect_target_plan.requirements(),
            )?;
        let effect_target_count = effect_target_plan.target_count;
        let offscreen_target_count = offscreen_target_plan.target_count;
        let offscreen_target_action_count = frame_resources
            .sync_offscreen_targets(
                device,
                memory_properties,
                frame_submission,
                &offscreen_target_plan,
            )?
            .len();
        let layer_alpha_mask_descriptors =
            NativeVulkanSceneLayerAlphaMaskDescriptorPlan::from_scene(
                resources,
                objects,
                &frame.layer_compositor,
            )?;
        native_vulkan_begin_scene_frame_command_buffer(device, slot_sync.command_buffer)?;
        let resource_prepare = native_vulkan_record_scene_mesh_resource_prepare_frame(
            frame_resources,
            NativeVulkanSceneMeshResourcePrepareContext {
                device,
                memory_properties,
                descriptor_heap_properties,
                command_buffer: slot_sync.command_buffer,
                frame_submission,
            },
            resources,
            frame,
            Some(&layer_alpha_mask_descriptors),
        )?;
        let layer_alpha_mask_heap_bind_count = layer_alpha_mask_descriptors.heap_bind_count;
        let layer_alpha_mask_resource_heap_action_count = frame_resources
            .sync_layer_alpha_mask_resource_heap(
                device,
                memory_properties,
                &layer_alpha_mask_descriptors,
                descriptor_heap_properties,
            )?
            .len();
        let layer_alpha_mask_resource_heap = frame_resources
            .current_layer_alpha_mask_resource_heap_frame_plan()
            .ok_or_else(|| {
                "scene prepare missing layer alpha-mask resource heap frame plan after sync"
                    .to_owned()
            })?;
        let layer_alpha_mask_heap_slice_count = layer_alpha_mask_resource_heap.heap_slice_count;
        let layer_alpha_mask_resource_descriptor_count =
            layer_alpha_mask_resource_heap.resource_descriptor_count;
        let layer_alpha_mask_sampler_descriptor_count =
            layer_alpha_mask_resource_heap.sampler_descriptor_count;
        let effect_texture_descriptors =
            frame_resources.effect_texture_descriptor_frame_plan(post_effect_graph)?;
        let effect_texture_descriptor_binding_count = effect_texture_descriptors.binding_count;
        let effect_uniform_gpu_buffer_action_count = frame_resources
            .sync_effect_uniform_gpu_buffers_recorded(
                device,
                memory_properties,
                slot_sync.command_buffer,
                frame_submission,
                frame,
                &effect_texture_descriptors,
            )?
            .len();
        let effect_resource_heap_action_count = frame_resources
            .sync_effect_resource_heap(
                device,
                memory_properties,
                frame,
                &effect_texture_descriptors,
                descriptor_heap_properties,
            )?
            .len();
        let effect_resource_heap = frame_resources
            .current_effect_resource_heap_frame_plan()
            .ok_or_else(|| {
                "scene prepare missing effect resource heap frame plan after sync".to_owned()
            })?;
        let effect_heap_slice_count = effect_resource_heap.heap_slice_count;
        let effect_resource_descriptor_count = effect_resource_heap.resource_descriptor_count;
        let effect_sampler_descriptor_count = effect_resource_heap.sampler_descriptor_count;
        native_vulkan_end_scene_frame_command_buffer(device, slot_sync.command_buffer)?;
        let prepare_submit = native_vulkan_submit_scene_prepare_commands2(
            device,
            queue,
            NativeVulkanScenePrepareSubmitContext {
                frame_submission,
                command_buffer: slot_sync.command_buffer,
                in_flight_fence: slot_sync.in_flight_fence,
            },
        )?;
        submitted_to_queue = true;
        unsafe {
            device
                .wait_for_fences(&[slot_sync.in_flight_fence], true, u64::MAX)
                .map_err(|err| format!("vkWaitForFences(scene cold resource prepare): {err:?}"))?;
        }
        frame_slots.complete_frame_submission(frame_submission)?;
        let _ = frame_resources.release_completed_frame_resources(device, frame_submission);
        let layer_alpha_mask_plan = native_vulkan_plan_scene_layer_alpha_mask_runtime_frame(
            frame_resources,
            &frame.layer_compositor,
            swapchain_extent,
        )?;
        let layer_alpha_mask_resource_binds =
            native_vulkan_plan_scene_layer_alpha_mask_resource_binds(
                frame_resources,
                &layer_alpha_mask_plan,
            )?;
        let layer_alpha_mask_token_schedule =
            native_vulkan_plan_scene_layer_alpha_mask_token_schedule(
                &layer_alpha_mask_plan,
                &layer_alpha_mask_resource_binds,
            )?;
        let layer_alpha_mask_generated_consumer_draws =
            native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_draws(
                &layer_alpha_mask_plan,
                &layer_alpha_mask_resource_binds,
                &layer_alpha_mask_token_schedule,
            )?;
        let layer_alpha_mask_generated_consumer_targets =
            native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_targets(
                &layer_alpha_mask_generated_consumer_draws,
                |object, target| {
                    native_vulkan_resolve_scene_layer_490_color_target(
                        frame_resources,
                        &target_formats,
                        swapchain_extent,
                        &frame.layer_compositor,
                        object,
                        target,
                    )
                },
            )?;
        let layer_alpha_mask_generated_consumer_pipelines =
            native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_pipelines_from_targets(
                &layer_alpha_mask_generated_consumer_draws,
                &layer_alpha_mask_generated_consumer_targets,
            )?;
        let mut layer_alpha_mask_pipeline_keys =
            layer_alpha_mask_plan.pipeline_warmup.cache_keys().to_vec();
        layer_alpha_mask_pipeline_keys.extend_from_slice(
            layer_alpha_mask_resource_binds
                .copy_back_pipelines
                .cache_keys(),
        );
        layer_alpha_mask_pipeline_keys
            .extend_from_slice(layer_alpha_mask_generated_consumer_pipelines.cache_keys());
        let pipeline_prepare =
            native_vulkan_prepare_scene_mesh_pipeline_cache_with_shader_catalog_and_extra_keys(
                device,
                frame_resources,
                &frame.graph,
                |target| target_formats.format(target),
                shader_catalog,
                &layer_alpha_mask_pipeline_keys,
            )?;
        let effect_pipeline_prepare =
            native_vulkan_prepare_scene_effect_pipeline_cache_with_target_formats(
                device,
                frame_resources,
                post_effect_graph,
                |target| {
                    effect_target_plan
                    .format(target)
                    .or_else(|effect_err| {
                        target_formats.format(target).map_err(|graph_err| {
                            format!(
                                "{effect_err}; scene graph target format fallback also failed: {graph_err}"
                            )
                        })
                    })
                },
                effect_shader_catalog,
            )?;
        Ok(NativeVulkanVulkanaliaScenePrepareSnapshot {
            resource_prepare: NativeVulkanVulkanaliaSceneResourcePrepareSnapshot::from_plan(
                &resource_prepare,
            ),
            pipeline_prepare: NativeVulkanVulkanaliaScenePipelinePrepareSnapshot::from_plan(
                &pipeline_prepare,
            ),
            effect_pipeline_prepare:
                NativeVulkanVulkanaliaSceneEffectPipelinePrepareSnapshot::from_plan(
                    &effect_pipeline_prepare,
                ),
            prepare_submit: NativeVulkanVulkanaliaScenePrepareSubmitSnapshot::from_plan(
                &prepare_submit,
            ),
            scene_shader_count: shader_catalog.shader_count(),
            graph_target_format_count: target_formats.target_format_count(),
            effect_target_count,
            effect_texture_descriptor_binding_count,
            effect_uniform_gpu_buffer_action_count,
            effect_resource_heap_action_count,
            effect_heap_slice_count,
            effect_resource_descriptor_count,
            effect_sampler_descriptor_count,
            layer_alpha_mask_heap_bind_count,
            layer_alpha_mask_resource_heap_action_count,
            layer_alpha_mask_heap_slice_count,
            layer_alpha_mask_resource_descriptor_count,
            layer_alpha_mask_sampler_descriptor_count,
            offscreen_target_count,
            offscreen_target_action_count,
            cold_prepare_wait: "vkWaitForFences only before present-frame loop",
            command_order: [
                "derive_effect_target_requirements",
                "sync_retained_offscreen_targets",
                "record_resource_prepare_command_buffer",
                "prepare_layer_alpha_mask_descriptors",
                "sync_layer_alpha_mask_resource_heap",
                "prepare_effect_texture_descriptors",
                "record_effect_uniform_buffer_uploads",
                "sync_effect_resource_heap",
                "queue_submit2_scene_prepare",
                "wait_scene_prepare_fence_cold_path",
                "release_completed_prepare_staging",
                "prepare_scene_mesh_pipeline_cache",
                "prepare_scene_effect_pipeline_cache",
            ],
        })
    })();

    if result.is_err() && !submitted_to_queue {
        let _ = frame_slots.abort_frame_submission(frame_submission);
        let _ = frame_resources.release_completed_frame_resources(device, frame_submission);
    }
    result
}

impl NativeVulkanVulkanaliaScenePipelinePrepareSnapshot {
    fn from_plan(plan: &NativeVulkanSceneMeshPipelinePreparePlan) -> Self {
        Self {
            target_format: plan.target_format.clone(),
            target_formats: plan.target_formats.clone(),
            target_format_count: plan.target_format_count,
            draw_count: plan.draw_count,
            cache_key_count: plan.cache_key_count,
            created_pipeline_count: plan.created_pipeline_count,
            reused_pipeline_count: plan.reused_pipeline_count,
            resource_descriptor_count: plan.resource_descriptor_count,
            sampler_descriptor_count: plan.sampler_descriptor_count,
            descriptor_model: plan.descriptor_model,
            command_order: plan.command_order,
        }
    }
}

impl NativeVulkanVulkanaliaSceneEffectPipelinePrepareSnapshot {
    fn from_plan(plan: &NativeVulkanSceneEffectPipelinePreparePlan) -> Self {
        Self {
            target_formats: plan.target_formats.clone(),
            target_format_count: plan.target_format_count,
            material_pass_count: plan.material_pass_count,
            cache_key_count: plan.cache_key_count,
            shader_artifact_count: plan.shader_artifact_count,
            created_pipeline_count: plan.created_pipeline_count,
            reused_pipeline_count: plan.reused_pipeline_count,
            resource_descriptor_count: plan.resource_descriptor_count,
            sampler_descriptor_count: plan.sampler_descriptor_count,
            descriptor_model: plan.descriptor_model,
            command_order: plan.command_order,
        }
    }
}

impl NativeVulkanVulkanaliaScenePrepareSubmitSnapshot {
    fn from_plan(plan: &NativeVulkanScenePrepareSubmitPlan) -> Self {
        Self {
            frame_slot: plan.frame_submission.frame_slot,
            submission_index: plan.frame_submission.submission_index,
            wait_stage: plan.wait_stage,
            signal_stage: plan.signal_stage,
            command_order: plan.command_order,
        }
    }
}

impl NativeVulkanVulkanaliaSceneResourcePrepareSnapshot {
    fn from_plan(plan: &NativeVulkanSceneMeshResourcePreparePlan) -> Self {
        Self {
            residency_command_count: plan.residency_command_count,
            material_uniform_gpu_buffer_action_count: plan.material_uniform_gpu_buffer_action_count,
            texture_descriptor_binding_count: plan.texture_descriptors.binding_count,
            resource_heap_action_count: plan.resource_heap_action_count,
            texture_image_action_count: plan.texture_image_action_count,
            gpu_buffer_action_count: plan.gpu_buffer_action_count,
            descriptor_model: plan.resource_heap.descriptor_model,
            resource_descriptor_count: plan.resource_heap.resource_descriptor_count,
            sampler_descriptor_count: plan.resource_heap.sampler_descriptor_count,
            command_order: plan.command_order,
        }
    }
}
