//! Runtime command-list contract for generated `CLIPPINGTARGET` consumer draws.
//!
//! References:
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `reverse-engineered/docs/exe/TODO.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;

use crate::engine::scene_engine::{
    SceneGraphPipelineClass, SceneGraphTarget, SceneLayerCompositorTarget, SceneObjectId,
};
use crate::renderer::native_vulkan::scene_backend::frame_resources::NativeVulkanSceneFrameResources;
use crate::renderer::native_vulkan::scene_backend::layer_alpha_mask_resource_heap::NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo;
use crate::renderer::native_vulkan::scene_backend::pipeline::NativeVulkanScenePipelineVertexLayout;
use crate::renderer::native_vulkan::scene_backend::texture_descriptors::NativeVulkanSceneTextureDescriptorVkFormat;

use super::NativeVulkanSceneLayerAlphaMaskTextureBindRole;
use super::consumer_draws::{
    GENERATED_CLIPPINGTARGET_SHADER,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawBindingPlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan,
};
use super::consumer_pipeline::{
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelineBindingPlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan,
};
use super::consumer_target::{
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetBindingPlan,
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetPlan,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan
{
    pub command_count: usize,
    pub warmed_pipeline_count: usize,
    pub descriptor_heap_bind_count: usize,
    pub target_scope_count: usize,
    pub pipeline_bind_count: usize,
    pub resource_heap_bind_count: usize,
    pub rt_method_8_indexed_draw_count: usize,
    pub commands: Vec<NativeVulkanSceneLayerAlphaMaskGeneratedConsumerCommandPlan>,
    pub command_order: [&'static str; 6],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskGeneratedConsumerCommandPlan
{
    pub consumer_draw_index: usize,
    pub command_index: usize,
    pub object: SceneObjectId,
    pub shader: &'static str,
    pub shader_combo_values: Vec<String>,
    pub source_mask: SceneGraphTarget,
    pub draw_receiver: SceneLayerCompositorTarget,
    pub color_target: SceneGraphTarget,
    pub target_format: NativeVulkanSceneTextureDescriptorVkFormat,
    pub target_format_label: &'static str,
    pub width: u32,
    pub height: u32,
    pub pipeline_class: SceneGraphPipelineClass,
    pub vertex_layout: NativeVulkanScenePipelineVertexLayout,
    pub heap_bind_index: usize,
    pub heap_slice_index: usize,
    pub base_resource_descriptor_index: usize,
    pub base_sampler_descriptor_index: usize,
    pub resource_descriptor_count: usize,
    pub texture_count: usize,
    pub shader_mappings: Vec<String>,
    pub material_source: &'static str,
    pub blend_byte_source: &'static str,
    pub geometry_source: &'static str,
    pub effective_alpha_formula: &'static str,
    pub pipeline_bind_count: usize,
    pub resource_heap_bind_count: usize,
    pub target_bind_count: usize,
    pub rt_method_8_indexed_draw_count: usize,
    pub draw_call: &'static str,
    pub command_order: [&'static str; 8],
}

impl NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan {
    pub(in crate::renderer::native_vulkan) fn from_draws_targets_pipelines_and_heap(
        consumer_draws: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan,
        targets: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetPlan,
        pipelines: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan,
        mut bind_info_for_heap_bind: impl FnMut(
            usize,
        ) -> Result<
            NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo,
            String,
        >,
        warmed_pipeline_count: usize,
    ) -> Result<Self, String> {
        if consumer_draws.bindings.is_empty() {
            return Ok(Self::empty());
        }
        if consumer_draws.consumer_draw_count != targets.consumer_draw_count
            || consumer_draws.consumer_draw_count != pipelines.consumer_draw_count
            || consumer_draws.bindings.len() != targets.target_binding_count
            || consumer_draws.bindings.len() != pipelines.pipeline_binding_count
        {
            return Err(format!(
                "scene layer alpha-mask generated consumer command-list expected matching draw/target/pipeline counts, got draws={} targets={}/{} pipelines={}/{}",
                consumer_draws.bindings.len(),
                targets.target_binding_count,
                targets.consumer_draw_count,
                pipelines.pipeline_binding_count,
                pipelines.consumer_draw_count
            ));
        }

        let mut commands = Vec::with_capacity(consumer_draws.bindings.len());
        for draw in &consumer_draws.bindings {
            let target = targets
                .target_for_consumer_draw(draw.consumer_draw_index)
                .ok_or_else(|| {
                    format!(
                        "scene layer alpha-mask generated consumer command {} has no target plan",
                        draw.command_index
                    )
                })?;
            let pipeline = pipelines
                .bindings
                .iter()
                .find(|pipeline| pipeline.consumer_draw_index == draw.consumer_draw_index)
                .ok_or_else(|| {
                    format!(
                        "scene layer alpha-mask generated consumer command {} has no pipeline plan",
                        draw.command_index
                    )
                })?;
            let bind_info =
                bind_info_for_heap_bind(draw.heap_bind_index).map_err(|err| {
                    format!(
                        "{err}; scene layer alpha-mask generated consumer command {} requires heap-bind {} bind info",
                        draw.command_index, draw.heap_bind_index
                    )
                })?;
            commands.push(NativeVulkanSceneLayerAlphaMaskGeneratedConsumerCommandPlan::from_draw_target_pipeline_and_heap(
                draw, target, pipeline, &bind_info,
            )?);
        }

        let pipeline_bind_count = commands
            .iter()
            .map(|command| command.pipeline_bind_count)
            .sum();
        let resource_heap_bind_count = commands
            .iter()
            .map(|command| command.resource_heap_bind_count)
            .sum();
        let rt_method_8_indexed_draw_count = commands
            .iter()
            .map(|command| command.rt_method_8_indexed_draw_count)
            .sum();

        Ok(Self {
            command_count: commands.len(),
            warmed_pipeline_count,
            descriptor_heap_bind_count: commands.len(),
            target_scope_count: commands.len(),
            pipeline_bind_count,
            resource_heap_bind_count,
            rt_method_8_indexed_draw_count,
            commands,
            command_order: generated_consumer_runtime_command_order(),
        })
    }

    fn empty() -> Self {
        Self {
            command_count: 0,
            warmed_pipeline_count: 0,
            descriptor_heap_bind_count: 0,
            target_scope_count: 0,
            pipeline_bind_count: 0,
            resource_heap_bind_count: 0,
            rt_method_8_indexed_draw_count: 0,
            commands: Vec::new(),
            command_order: generated_consumer_runtime_command_order(),
        }
    }

    pub(in crate::renderer::native_vulkan) fn command_for_consumer_draw(
        &self,
        consumer_draw_index: usize,
    ) -> Option<&NativeVulkanSceneLayerAlphaMaskGeneratedConsumerCommandPlan> {
        self.commands
            .iter()
            .find(|command| command.consumer_draw_index == consumer_draw_index)
    }
}

impl NativeVulkanSceneLayerAlphaMaskGeneratedConsumerCommandPlan {
    fn from_draw_target_pipeline_and_heap(
        draw: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawBindingPlan,
        target: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetBindingPlan,
        pipeline: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelineBindingPlan,
        bind_info: &NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo,
    ) -> Result<Self, String> {
        validate_draw_target_pipeline_identity(draw, target, pipeline)?;
        validate_generated_consumer_heap_bind(draw, pipeline, bind_info)?;
        Ok(Self {
            consumer_draw_index: draw.consumer_draw_index,
            command_index: draw.command_index,
            object: draw.object,
            shader: GENERATED_CLIPPINGTARGET_SHADER,
            shader_combo_values: pipeline
                .shader_combo_values
                .iter()
                .map(|combo| format!("{}={}", combo.name, combo.value))
                .collect(),
            source_mask: draw.source_mask,
            draw_receiver: target.draw_receiver,
            color_target: target.color_target,
            target_format: target.target_format,
            target_format_label: target.target_format_label,
            width: target.width,
            height: target.height,
            pipeline_class: pipeline.pipeline_class,
            vertex_layout: pipeline.vertex_layout,
            heap_bind_index: bind_info.heap_bind_index,
            heap_slice_index: pipeline.heap_slice_index,
            base_resource_descriptor_index: pipeline.base_resource_descriptor_index,
            base_sampler_descriptor_index: pipeline.base_sampler_descriptor_index,
            resource_descriptor_count: pipeline.resource_descriptor_count,
            texture_count: pipeline.texture_count,
            shader_mappings: pipeline.shader_mappings.clone(),
            material_source: pipeline.material_source,
            blend_byte_source: pipeline.blend_byte_source,
            geometry_source: target.geometry_source,
            effective_alpha_formula: "src.a * FullAlphaMask.r with translucent src-alpha/inv-src-alpha blend",
            pipeline_bind_count: 1,
            resource_heap_bind_count: 1,
            target_bind_count: 1,
            rt_method_8_indexed_draw_count: 1,
            draw_call: "[layer+0x490].vtable+0x40",
            command_order: [
                "require_warmed_genericimage4_clippingtarget_pipeline_variant",
                "resolve_generated_clippingtarget_resource_heap_bind",
                "resolve_layer_0x490_current_color_target_scope",
                "preserve_generated_material_0x428_and_blend_0x1f0",
                "bind_generated_clippingtarget_pipeline_variant",
                "bind_generated_clippingtarget_resource_heap_ext",
                "bind_layer_0x490_rt_method_8_geometry",
                "record_layer_0x490_generated_indexed_draw",
            ],
        })
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_runtime_commands(
    frame_resources: &NativeVulkanSceneFrameResources,
    consumer_draws: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan,
    targets: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetPlan,
    pipelines: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelinePlan,
) -> Result<NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan, String> {
    if consumer_draws.bindings.is_empty() {
        return Ok(NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan::empty());
    }
    for cache_key in pipelines.cache_keys() {
        frame_resources.cached_mesh_pipeline(cache_key).map_err(|err| {
            format!(
                "{err}; scene layer alpha-mask runtime requires generated CLIPPINGTARGET pipeline warmup before command-list assembly"
            )
        })?;
    }
    NativeVulkanSceneLayerAlphaMaskGeneratedConsumerRuntimeCommandPlan::from_draws_targets_pipelines_and_heap(
        consumer_draws,
        targets,
        pipelines,
        |heap_bind_index| frame_resources.layer_alpha_mask_resource_heap_bind_info(heap_bind_index),
        pipelines.cache_keys().len(),
    )
}

fn validate_draw_target_pipeline_identity(
    draw: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawBindingPlan,
    target: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetBindingPlan,
    pipeline: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelineBindingPlan,
) -> Result<(), String> {
    if draw.consumer_draw_index != target.consumer_draw_index
        || draw.consumer_draw_index != pipeline.consumer_draw_index
        || draw.command_index != target.command_index
        || draw.command_index != pipeline.command_index
        || draw.object != target.object
        || draw.object != pipeline.object
    {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {} draw/target/pipeline identity mismatch",
            draw.command_index
        ));
    }
    if draw.source_mask != SceneGraphTarget::FullAlphaMask
        || pipeline.source_mask != SceneGraphTarget::FullAlphaMask
    {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {} must sample FullAlphaMask",
            draw.command_index
        ));
    }
    if draw.target != SceneLayerCompositorTarget::LayerTarget490
        || target.draw_receiver != SceneLayerCompositorTarget::LayerTarget490
        || pipeline.target != SceneLayerCompositorTarget::LayerTarget490
    {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {} must draw through LayerTarget490",
            draw.command_index
        ));
    }
    if pipeline.shader != GENERATED_CLIPPINGTARGET_SHADER {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {} requires {}, got {}",
            draw.command_index, GENERATED_CLIPPINGTARGET_SHADER, pipeline.shader
        ));
    }
    let combo_labels = pipeline
        .shader_combo_values
        .iter()
        .map(|combo| (combo.name.as_str(), combo.value))
        .collect::<Vec<_>>();
    if combo_labels != [("CLIPPINGTARGET", 1), ("CLIPPINGUVS", 1)] {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {} requires CLIPPINGTARGET=1 and CLIPPINGUVS=1, got {:?}",
            draw.command_index, combo_labels
        ));
    }
    if target.target_format != pipeline.target_format
        || target.pipeline_class != pipeline.pipeline_class
        || target.vertex_layout != pipeline.vertex_layout
    {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {} target and pipeline variants disagree",
            draw.command_index
        ));
    }
    if target.pipeline_class != SceneGraphPipelineClass::Mesh
        && target.pipeline_class != SceneGraphPipelineClass::PuppetSkinning
    {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {} requires mesh/subdraw pipeline class, got {:?}",
            draw.command_index, target.pipeline_class
        ));
    }
    Ok(())
}

fn validate_generated_consumer_heap_bind(
    draw: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawBindingPlan,
    pipeline: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerPipelineBindingPlan,
    bind_info: &NativeVulkanSceneLayerAlphaMaskResourceHeapBindInfo,
) -> Result<(), String> {
    if bind_info.role != NativeVulkanSceneLayerAlphaMaskTextureBindRole::GeneratedClippingTarget {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {} requires GeneratedClippingTarget heap bind, got {:?}",
            draw.command_index, bind_info.role
        ));
    }
    if bind_info.object != draw.object {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {} object mismatch: draw {:?}, heap {:?}",
            draw.command_index, draw.object, bind_info.object
        ));
    }
    if bind_info.shader != GENERATED_CLIPPINGTARGET_SHADER {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {} heap shader mismatch: expected {}, heap {}",
            draw.command_index, GENERATED_CLIPPINGTARGET_SHADER, bind_info.shader
        ));
    }
    if bind_info.heap_bind_index != draw.heap_bind_index
        || bind_info.heap_bind_index != pipeline.heap_bind_index
    {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {} heap-bind mismatch: draw {} pipeline {} heap {}",
            draw.command_index,
            draw.heap_bind_index,
            pipeline.heap_bind_index,
            bind_info.heap_bind_index
        ));
    }
    if bind_info.heap_slice_index != draw.heap_slice_index
        || bind_info.heap_slice_index != pipeline.heap_slice_index
    {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {} heap-slice mismatch",
            draw.command_index
        ));
    }
    if bind_info.base_resource_descriptor_index != draw.base_resource_descriptor_index
        || bind_info.base_resource_descriptor_index != pipeline.base_resource_descriptor_index
    {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {} resource descriptor base mismatch",
            draw.command_index
        ));
    }
    if bind_info.base_sampler_descriptor_index != draw.base_sampler_descriptor_index
        || bind_info.base_sampler_descriptor_index != pipeline.base_sampler_descriptor_index
    {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {} sampler descriptor base mismatch",
            draw.command_index
        ));
    }
    if bind_info.texture_count != 2
        || draw.texture_count != 2
        || pipeline.texture_count != 2
        || bind_info.resource_descriptor_count < 2
    {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {} requires slot0 source + slot8 FullAlphaMask sampled images, got heap textures={} resources={}",
            draw.command_index, bind_info.texture_count, bind_info.resource_descriptor_count
        ));
    }
    if bind_info.shader_mappings != draw.shader_mappings
        || bind_info.shader_mappings != pipeline.shader_mappings
    {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {} shader heap mappings drifted",
            draw.command_index
        ));
    }
    Ok(())
}

fn generated_consumer_runtime_command_order() -> [&'static str; 6] {
    [
        "require_warmed_genericimage4_clippingtarget_pipelines",
        "resolve_generated_clippingtarget_heap_binds",
        "join_generated_draw_target_pipeline_contracts",
        "preserve_token1_effective_alpha_formula",
        "build_generated_consumer_command_plan",
        "defer_geometry_and_uniform_recording_to_rt_method_8_recorder",
    ]
}

#[cfg(test)]
#[path = "consumer_command_tests.rs"]
mod tests;
