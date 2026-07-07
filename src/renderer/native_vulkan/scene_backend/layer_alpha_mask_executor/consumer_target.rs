//! Target contracts for generated `CLIPPINGTARGET` consumer draws.
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/clipping-pipeline.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/renderer_rd/pipeline_hash_map_rd.h`

use std::collections::BTreeSet;

use serde::Serialize;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SceneGraphPipelineClass, SceneGraphTarget, SceneLayerCompositorTarget, SceneObjectId,
};
use crate::renderer::native_vulkan::scene_backend::pipeline::NativeVulkanScenePipelineVertexLayout;
use crate::renderer::native_vulkan::scene_backend::texture_descriptors::NativeVulkanSceneTextureDescriptorVkFormat;

use super::consumer_draws::NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan;
use super::rt_method8::LAYER_490_RT_METHOD8_GEOMETRY_SOURCE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskLayerTargetBinding {
    pub object: SceneObjectId,
    pub layer_target: SceneLayerCompositorTarget,
    pub color_target: SceneGraphTarget,
    pub format: vk::Format,
    pub width: u32,
    pub height: u32,
    pub pipeline_class: SceneGraphPipelineClass,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetPlan
{
    pub consumer_draw_count: usize,
    pub target_binding_count: usize,
    pub color_target_count: usize,
    pub bindings: Vec<NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetBindingPlan>,
    pub command_order: [&'static str; 6],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetBindingPlan
{
    pub consumer_draw_index: usize,
    pub command_index: usize,
    pub object: SceneObjectId,
    pub draw_receiver: SceneLayerCompositorTarget,
    pub color_target: SceneGraphTarget,
    pub target_format: NativeVulkanSceneTextureDescriptorVkFormat,
    pub target_format_label: &'static str,
    pub width: u32,
    pub height: u32,
    pub pipeline_class: SceneGraphPipelineClass,
    pub vertex_layout: NativeVulkanScenePipelineVertexLayout,
    pub geometry_source: &'static str,
    pub command_order: [&'static str; 6],
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_plan_scene_layer_alpha_mask_generated_consumer_targets(
    consumer_draws: &NativeVulkanSceneLayerAlphaMaskGeneratedConsumerDrawRuntimePlan,
    mut target_binding: impl FnMut(
        SceneObjectId,
        SceneLayerCompositorTarget,
    )
        -> Result<NativeVulkanSceneLayerAlphaMaskLayerTargetBinding, String>,
) -> Result<NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetPlan, String> {
    if consumer_draws.bindings.is_empty() {
        return Ok(NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetPlan::empty());
    }

    let mut bindings = Vec::with_capacity(consumer_draws.bindings.len());
    for consumer in &consumer_draws.bindings {
        let binding = target_binding(consumer.object, consumer.target).map_err(|err| {
            format!(
                "{err}; scene layer alpha-mask generated consumer command {} requires a formal LayerTarget490 color target resolver",
                consumer.command_index
            )
        })?;
        bindings.push(consumer_target_binding(
            consumer.consumer_draw_index,
            consumer.command_index,
            consumer.object,
            consumer.target,
            binding,
        )?);
    }

    Ok(
        NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetPlan::from_bindings(
            consumer_draws.consumer_draw_count,
            bindings,
        ),
    )
}

impl NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetPlan {
    pub(in crate::renderer::native_vulkan) fn target_for_consumer_draw(
        &self,
        consumer_draw_index: usize,
    ) -> Option<&NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetBindingPlan> {
        self.bindings
            .iter()
            .find(|binding| binding.consumer_draw_index == consumer_draw_index)
    }

    fn empty() -> Self {
        Self {
            consumer_draw_count: 0,
            target_binding_count: 0,
            color_target_count: 0,
            bindings: Vec::new(),
            command_order: generated_consumer_target_command_order(),
        }
    }

    fn from_bindings(
        consumer_draw_count: usize,
        bindings: Vec<NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetBindingPlan>,
    ) -> Self {
        let color_target_count = bindings
            .iter()
            .map(|binding| binding.color_target)
            .collect::<BTreeSet<_>>()
            .len();
        Self {
            consumer_draw_count,
            target_binding_count: bindings.len(),
            color_target_count,
            bindings,
            command_order: generated_consumer_target_command_order(),
        }
    }
}

fn consumer_target_binding(
    consumer_draw_index: usize,
    command_index: usize,
    object: SceneObjectId,
    expected_target: SceneLayerCompositorTarget,
    binding: NativeVulkanSceneLayerAlphaMaskLayerTargetBinding,
) -> Result<NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetBindingPlan, String> {
    validate_target_identity(command_index, object, expected_target, binding)?;
    let target_format = NativeVulkanSceneTextureDescriptorVkFormat::from_vk_format(binding.format)
        .map_err(|err| {
            format!(
                "{err}; scene layer alpha-mask generated consumer command {command_index} target {:?}",
                binding.color_target
            )
        })?;
    validate_generated_consumer_color_format(command_index, target_format)?;
    let vertex_layout = generated_consumer_vertex_layout(command_index, binding.pipeline_class)?;
    if binding.width == 0 || binding.height == 0 {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {command_index} color target {:?} has zero extent {}x{}",
            binding.color_target, binding.width, binding.height
        ));
    }
    Ok(
        NativeVulkanSceneLayerAlphaMaskGeneratedConsumerTargetBindingPlan {
            consumer_draw_index,
            command_index,
            object,
            draw_receiver: binding.layer_target,
            color_target: binding.color_target,
            target_format,
            target_format_label: scene_texture_vk_format_label(target_format),
            width: binding.width,
            height: binding.height,
            pipeline_class: binding.pipeline_class,
            vertex_layout,
            geometry_source: LAYER_490_RT_METHOD8_GEOMETRY_SOURCE,
            command_order: [
                "resolve_layer_0x490_rt_draw_receiver",
                "keep_color_target_separate_from_layer_receiver",
                "read_current_layer_color_target_format",
                "validate_generated_consumer_color_attachment_format",
                "select_scene_mesh_vertex_layout_for_layer_0x490",
                "preserve_0x14020b15e_geometry_creation_site",
            ],
        },
    )
}

fn validate_target_identity(
    command_index: usize,
    object: SceneObjectId,
    expected_target: SceneLayerCompositorTarget,
    binding: NativeVulkanSceneLayerAlphaMaskLayerTargetBinding,
) -> Result<(), String> {
    if binding.object != object {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {command_index} target object mismatch: command {:?}, resolver {:?}",
            object, binding.object
        ));
    }
    if expected_target != SceneLayerCompositorTarget::LayerTarget490
        || binding.layer_target != SceneLayerCompositorTarget::LayerTarget490
    {
        return Err(format!(
            "scene layer alpha-mask generated consumer command {command_index} must resolve LayerTarget490, got command {:?} resolver {:?}",
            expected_target, binding.layer_target
        ));
    }
    Ok(())
}

fn validate_generated_consumer_color_format(
    command_index: usize,
    format: NativeVulkanSceneTextureDescriptorVkFormat,
) -> Result<(), String> {
    match format {
        NativeVulkanSceneTextureDescriptorVkFormat::R8G8B8A8Unorm
        | NativeVulkanSceneTextureDescriptorVkFormat::B8G8R8A8Unorm
        | NativeVulkanSceneTextureDescriptorVkFormat::R16G16B16A16Sfloat => Ok(()),
        format => Err(format!(
            "scene layer alpha-mask generated consumer command {command_index} requires a color layer target format, got {:?}",
            format
        )),
    }
}

fn generated_consumer_vertex_layout(
    command_index: usize,
    pipeline_class: SceneGraphPipelineClass,
) -> Result<NativeVulkanScenePipelineVertexLayout, String> {
    match pipeline_class {
        SceneGraphPipelineClass::Mesh | SceneGraphPipelineClass::PuppetSkinning => {
            Ok(NativeVulkanScenePipelineVertexLayout::SceneMeshV0)
        }
        pipeline_class => Err(format!(
            "scene layer alpha-mask generated consumer command {command_index} requires mesh/subdraw geometry, got {:?}",
            pipeline_class
        )),
    }
}

fn scene_texture_vk_format_label(
    format: NativeVulkanSceneTextureDescriptorVkFormat,
) -> &'static str {
    match format {
        NativeVulkanSceneTextureDescriptorVkFormat::R16G16B16A16Sfloat => "R16G16B16A16_SFLOAT",
        NativeVulkanSceneTextureDescriptorVkFormat::R16G16Sfloat => "R16G16_SFLOAT",
        NativeVulkanSceneTextureDescriptorVkFormat::R8G8B8A8Unorm => "R8G8B8A8_UNORM",
        NativeVulkanSceneTextureDescriptorVkFormat::B8G8R8A8Unorm => "B8G8R8A8_UNORM",
        NativeVulkanSceneTextureDescriptorVkFormat::R16Sfloat => "R16_SFLOAT",
        NativeVulkanSceneTextureDescriptorVkFormat::R8Unorm => "R8_UNORM",
    }
}

fn generated_consumer_target_command_order() -> [&'static str; 6] {
    [
        "read_generated_clippingtarget_draw_contracts",
        "resolve_layer_0x490_color_target",
        "validate_color_target_format_and_extent",
        "preserve_layer_0x490_rt_method_8_receiver",
        "select_mesh_or_puppet_pipeline_class",
        "produce_generated_consumer_target_plan",
    ]
}

#[cfg(test)]
#[path = "consumer_target_tests.rs"]
mod tests;
