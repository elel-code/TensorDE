//! Vulkan graphics-pipeline fixed state and dynamic-rendering metadata.
//!
//! Local-read metadata is accepted only as an already validated typed plan;
//! scene graph execution remains responsible for proving and recording the
//! matching dynamic-rendering scope.

use super::super::local_read::SceneLocalReadPipelineMetadata;
use super::blend::scene_color_blend_attachment;
use super::*;

#[allow(clippy::too_many_arguments)]
pub(super) fn create_graphics_pipeline(
    device: &Device,
    target_format: vk::Format,
    stages: [vk::PipelineShaderStageCreateInfo; 2],
    blend: SceneGpuBlend,
    cull_mode: SceneCullMode,
    color_write_mask: SceneColorWriteMask,
    advanced_source_premultiplied: bool,
    advanced_blend_overlap: vk::BlendOverlapEXT,
    samples: ScenePipelineSamples,
    topology: vk::PrimitiveTopology,
    local_read_metadata: Option<&SceneLocalReadPipelineMetadata<'_>>,
) -> Result<vk::Pipeline, String> {
    if local_read_metadata.is_some() && blend.requires_advanced_operation() {
        return Err(
            "scene local-read pipeline does not have a proven advanced-blend attachment contract"
                .to_owned(),
        );
    }
    if local_read_metadata.is_some() && samples != ScenePipelineSamples::Single {
        return Err(
            "scene local-read pipeline does not have a proven multisampled attachment contract"
                .to_owned(),
        );
    }

    let binding = vk::VertexInputBindingDescription::builder()
        .binding(0)
        .stride(super::super::SCENE_MESH_VERTEX_STRIDE_BYTES)
        .input_rate(vk::VertexInputRate::VERTEX)
        .build();
    let attributes = [
        vertex_attribute(0, vk::Format::R32G32_SFLOAT, 0),
        vertex_attribute(1, vk::Format::R32G32_SFLOAT, 8),
        vertex_attribute(2, vk::Format::R32_SFLOAT, 16),
        vertex_attribute(3, vk::Format::R32G32B32A32_UINT, 20),
        vertex_attribute(4, vk::Format::R32G32B32A32_SFLOAT, 36),
    ];
    let bindings = [binding];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::builder()
        .vertex_binding_descriptions(&bindings)
        .vertex_attribute_descriptions(&attributes)
        .build();
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
        .topology(topology)
        .build();
    let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
        .viewport_count(1)
        .scissor_count(1)
        .build();
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state = vk::PipelineDynamicStateCreateInfo::builder()
        .dynamic_states(&dynamic_states)
        .build();
    let rasterization = vk::PipelineRasterizationStateCreateInfo::builder()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(scene_vk_cull_mode(cull_mode))
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0)
        .build();
    let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
        .rasterization_samples(samples.rasterization_samples())
        .alpha_to_coverage_enable(blend == SceneGpuBlend::AlphaToCoverage)
        .build();

    let active_color_attachment = scene_color_blend_attachment(blend, color_write_mask);
    let default_color_attachments = [active_color_attachment];
    let local_color_attachments = local_read_metadata
        .map(|metadata| metadata.color_blend_attachments(active_color_attachment));
    let color_attachments = local_color_attachments
        .as_deref()
        .unwrap_or(&default_color_attachments);
    let mut advanced_blend = vk::PipelineColorBlendAdvancedStateCreateInfoEXT::builder()
        .src_premultiplied(advanced_source_premultiplied)
        .dst_premultiplied(false)
        .blend_overlap(advanced_blend_overlap)
        .build();
    let mut color_blend_builder =
        vk::PipelineColorBlendStateCreateInfo::builder().attachments(color_attachments);
    if blend.requires_advanced_operation() {
        color_blend_builder = color_blend_builder.push_next(&mut advanced_blend);
    }
    let color_blend = color_blend_builder.build();

    let default_color_attachment_formats = [target_format];
    let color_attachment_formats = local_read_metadata
        .map(SceneLocalReadPipelineMetadata::color_attachment_formats)
        .unwrap_or(&default_color_attachment_formats);
    let mut rendering_info = vk::PipelineRenderingCreateInfo::builder()
        .color_attachment_formats(color_attachment_formats)
        .build();
    let mut pipeline_flags2 = vk::PipelineCreateFlags2CreateInfo::builder()
        .flags(vk::PipelineCreateFlags2::DESCRIPTOR_HEAP_EXT)
        .build();
    let mut attachment_location_info =
        local_read_metadata.map(SceneLocalReadPipelineMetadata::attachment_location_info);
    let mut input_attachment_index_info =
        local_read_metadata.map(SceneLocalReadPipelineMetadata::input_attachment_index_info);
    let mut pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization)
        .multisample_state(&multisample)
        .color_blend_state(&color_blend)
        .dynamic_state(&dynamic_state)
        .layout(vk::PipelineLayout::null())
        .render_pass(vk::RenderPass::null())
        .subpass(0)
        .push_next(&mut rendering_info)
        .push_next(&mut pipeline_flags2);
    if let Some(info) = attachment_location_info.as_mut() {
        pipeline_info = pipeline_info.push_next(info);
    }
    if let Some(info) = input_attachment_index_info.as_mut() {
        pipeline_info = pipeline_info.push_next(info);
    }
    let pipeline_info = pipeline_info.build();
    let (pipelines, _success_code) = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
    }
    .map_err(|err| format!("vkCreateGraphicsPipelines(vulkanalia scene): {err:?}"))?;
    Ok(pipelines[0])
}

fn vertex_attribute(
    location: u32,
    format: vk::Format,
    offset: u32,
) -> vk::VertexInputAttributeDescription {
    vk::VertexInputAttributeDescription::builder()
        .location(location)
        .binding(0)
        .format(format)
        .offset(offset)
        .build()
}

pub(super) fn scene_vk_cull_mode(cull_mode: SceneCullMode) -> vk::CullModeFlags {
    match cull_mode {
        SceneCullMode::None => vk::CullModeFlags::NONE,
        SceneCullMode::Normal => vk::CullModeFlags::BACK,
    }
}
