//! Retained native descriptor-heap pipelines for decoded video presentation.

use super::*;
use vulkan_renderer::descriptor_heap_element_index;

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_create_decoded_image_present_pipeline_resources(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
) -> Result<VulkanaliaDecodedImagePresentPipelineResources, String> {
    validate_pipeline_input(extent, descriptor_heap_plan)?;
    let descriptor_push = decoded_image_descriptor_push(
        descriptor_heap_plan,
        DECODED_PRESENT_BINDINGS,
        DECODED_PRESENT_PUSH_BYTES,
        None,
    )?;
    let vertex_module = native_vulkan_vulkanalia_create_shader_module(
        device,
        DECODED_PRESENT_VERTEX_SPIRV,
        "decoded present vertex",
    )?;
    let result = (|| {
        let fragment_module = native_vulkan_vulkanalia_create_shader_module(
            device,
            DECODED_PRESENT_FRAGMENT_SPIRV,
            "decoded present fragment",
        )?;
        let result = (|| {
            let pipeline = create_fullscreen_pipeline(
                device,
                target_format,
                extent,
                vertex_module,
                fragment_module,
            )?;
            let scene_video_layer = match create_scene_video_layer_pipeline(
                device,
                target_format,
                extent,
                descriptor_heap_plan,
            ) {
                Ok(resources) => resources,
                Err(error) => {
                    unsafe { device.destroy_pipeline(pipeline, None) };
                    return Err(error);
                }
            };
            Ok(VulkanaliaDecodedImagePresentPipelineResources {
                pipeline,
                descriptor_push,
                scene_video_layer,
                snapshot: NativeVulkanVulkanaliaDecodedImagePresentPipelineSnapshot {
                    binding: "vulkanalia",
                    route: "decoded-image-dynamic-rendering-present-pipeline",
                    target_format: format!("{target_format:?}"),
                    extent: (extent.width, extent.height),
                    shader_modules_created: true,
                    pipeline_layout_null: true,
                    pipeline_created: true,
                    render_pass_compatibility: "dynamic-rendering-no-render-pass",
                    primitive_topology: "fullscreen-triangle",
                    vertex_shader_model: "native Slang SV_VertexID fullscreen triangle",
                    fragment_shader_model: "native Slang Y/UV Texture2DArray plus sampler descriptor handles and instance layer selection",
                    descriptor_heap_only: true,
                    descriptor_model: "VK_EXT_descriptor_heap",
                    native_descriptor_push_enabled: true,
                    descriptor_heap_plane_sampler_enabled: true,
                    descriptor_heap_pipeline_flag_enabled: true,
                    uses_pipeline_rendering_create_info: true,
                    uses_dynamic_rendering: true,
                    uses_plane_sampler_descriptors: true,
                    ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
                },
            })
        })();
        unsafe { device.destroy_shader_module(fragment_module, None) };
        result
    })();
    unsafe { device.destroy_shader_module(vertex_module, None) };
    result
}

fn validate_pipeline_input(
    extent: vk::Extent2D,
    plan: &NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
) -> Result<(), String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("decoded image present pipeline requires non-zero extent".to_owned());
    }
    if !plan.backend_ready {
        return Err(format!(
            "decoded image present pipeline requires a ready VK_EXT_descriptor_heap plan: {:?}",
            plan.blocking_reason
        ));
    }
    if plan.image_count != 2 {
        return Err(format!(
            "decoded image present pipeline requires exactly two plane descriptors, found {}",
            plan.image_count
        ));
    }
    Ok(())
}

fn create_fullscreen_pipeline(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
    vertex_module: vk::ShaderModule,
    fragment_module: vk::ShaderModule,
) -> Result<vk::Pipeline, String> {
    let entry = b"main\0";
    let stages = shader_stages(vertex_module, fragment_module, entry);
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::builder().build();
    let input_assembly = triangle_list_input_assembly();
    let (viewports, scissors) = fixed_viewport_and_scissor(extent);
    let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
        .viewports(&viewports)
        .scissors(&scissors)
        .build();
    let rasterization = rasterization_state();
    let multisample = multisample_state();
    let color_attachment = color_attachment_state(false);
    create_graphics_pipeline(
        device,
        target_format,
        &stages,
        &vertex_input,
        &input_assembly,
        &viewport_state,
        &rasterization,
        &multisample,
        &color_attachment,
        "decoded present dynamic rendering",
    )
}

fn create_scene_video_layer_pipeline(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
) -> Result<VulkanaliaDecodedImageSceneVideoLayerPipelineResources, String> {
    let descriptor_push = decoded_image_descriptor_push(
        descriptor_heap_plan,
        DECODED_SCENE_VIDEO_BINDINGS,
        DECODED_SCENE_VIDEO_PUSH_BYTES,
        Some(extent),
    )?;
    let vertex_module = native_vulkan_vulkanalia_create_shader_module(
        device,
        DECODED_SCENE_VIDEO_VERTEX_SPIRV,
        "decoded scene video layer vertex",
    )?;
    let result = (|| {
        let fragment_module = native_vulkan_vulkanalia_create_shader_module(
            device,
            DECODED_SCENE_VIDEO_FRAGMENT_SPIRV,
            "decoded scene video layer fragment",
        )?;
        let result = (|| {
            let entry = b"main\0";
            let stages = shader_stages(vertex_module, fragment_module, entry);
            let binding = vk::VertexInputBindingDescription::builder()
                .binding(0)
                .stride(DECODED_IMAGE_SCENE_VIDEO_LAYER_VERTEX_STRIDE_BYTES)
                .input_rate(vk::VertexInputRate::VERTEX)
                .build();
            let attributes = [
                vertex_attribute(0, vk::Format::R32G32_SFLOAT, 0),
                vertex_attribute(1, vk::Format::R32G32_SFLOAT, 8),
                vertex_attribute(2, vk::Format::R32_SFLOAT, 16),
            ];
            let bindings = [binding];
            let vertex_input = vk::PipelineVertexInputStateCreateInfo::builder()
                .vertex_binding_descriptions(&bindings)
                .vertex_attribute_descriptions(&attributes)
                .build();
            let input_assembly = triangle_list_input_assembly();
            let (viewports, scissors) = fixed_viewport_and_scissor(extent);
            let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
                .viewports(&viewports)
                .scissors(&scissors)
                .build();
            let rasterization = rasterization_state();
            let multisample = multisample_state();
            let color_attachment = color_attachment_state(true);
            let pipeline = create_graphics_pipeline(
                device,
                target_format,
                &stages,
                &vertex_input,
                &input_assembly,
                &viewport_state,
                &rasterization,
                &multisample,
                &color_attachment,
                "decoded scene video layer",
            )?;
            Ok(VulkanaliaDecodedImageSceneVideoLayerPipelineResources {
                pipeline,
                descriptor_push,
            })
        })();
        unsafe { device.destroy_shader_module(fragment_module, None) };
        result
    })();
    unsafe { device.destroy_shader_module(vertex_module, None) };
    result
}

fn decoded_image_descriptor_push(
    plan: &NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    bindings: &[DecodedImageDescriptorBinding],
    push_bytes: u32,
    extent: Option<vk::Extent2D>,
) -> Result<Vec<u8>, String> {
    if bindings.len() != 4 || !push_bytes.is_multiple_of(4) {
        return Err("decoded image shader has an invalid native push ABI".to_owned());
    }
    let mut push = vec![0; push_bytes as usize];
    if let Some(extent) = extent {
        push[0..4].copy_from_slice(&(extent.width as f32).to_le_bytes());
        push[4..8].copy_from_slice(&(extent.height as f32).to_le_bytes());
    }
    for binding in bindings {
        let descriptor = binding.register as usize;
        let (offset, descriptor_size) = match binding.kind {
            DecodedImageDescriptorKind::SampledImage => (
                *plan
                    .image_descriptor_offsets
                    .get(descriptor)
                    .ok_or_else(|| format!("decoded image descriptor {descriptor} is missing"))?,
                plan.image_descriptor_size,
            ),
            DecodedImageDescriptorKind::Sampler => (
                *plan
                    .sampler_descriptor_offsets
                    .get(descriptor)
                    .ok_or_else(|| format!("decoded sampler descriptor {descriptor} is missing"))?,
                plan.sampler_descriptor_size,
            ),
        };
        let element = descriptor_heap_element_index(offset, descriptor_size)
            .map_err(|error| format!("resolve decoded image heap element: {error}"))?;
        let start = binding.push_offset as usize;
        let end = start
            .checked_add(4)
            .filter(|end| *end <= push.len())
            .ok_or_else(|| "decoded image descriptor push offset exceeds ABI".to_owned())?;
        push[start..end].copy_from_slice(&element.to_le_bytes());
    }
    Ok(push)
}

fn shader_stages(
    vertex: vk::ShaderModule,
    fragment: vk::ShaderModule,
    entry: &[u8],
) -> [vk::PipelineShaderStageCreateInfo; 2] {
    [
        vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex)
            .name(entry)
            .build(),
        vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fragment)
            .name(entry)
            .build(),
    ]
}

fn vertex_attribute(location: u32, format: vk::Format, offset: u32) -> vk::VertexInputAttributeDescription {
    vk::VertexInputAttributeDescription::builder()
        .location(location)
        .binding(0)
        .format(format)
        .offset(offset)
        .build()
}

fn triangle_list_input_assembly() -> vk::PipelineInputAssemblyStateCreateInfo {
    vk::PipelineInputAssemblyStateCreateInfo::builder()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
        .build()
}

fn fixed_viewport_and_scissor(extent: vk::Extent2D) -> ([vk::Viewport; 1], [vk::Rect2D; 1]) {
    ([vk::Viewport::builder()
        .x(0.0)
        .y(0.0)
        .width(extent.width as f32)
        .height(extent.height as f32)
        .min_depth(0.0)
        .max_depth(1.0)
        .build()], [vk::Rect2D::builder()
        .offset(vk::Offset2D { x: 0, y: 0 })
        .extent(extent)
        .build()])
}

fn rasterization_state() -> vk::PipelineRasterizationStateCreateInfo {
    vk::PipelineRasterizationStateCreateInfo::builder()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0)
        .build()
}

fn multisample_state() -> vk::PipelineMultisampleStateCreateInfo {
    vk::PipelineMultisampleStateCreateInfo::builder()
        .rasterization_samples(vk::SampleCountFlags::_1)
        .build()
}

fn color_attachment_state(blend: bool) -> vk::PipelineColorBlendAttachmentState {
    let builder = vk::PipelineColorBlendAttachmentState::builder()
        .color_write_mask(
            vk::ColorComponentFlags::R
                | vk::ColorComponentFlags::G
                | vk::ColorComponentFlags::B
                | vk::ColorComponentFlags::A,
        )
        .blend_enable(blend);
    if blend {
        builder
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD)
            .build()
    } else {
        builder.build()
    }
}

#[allow(clippy::too_many_arguments)]
fn create_graphics_pipeline(
    device: &Device,
    target_format: vk::Format,
    stages: &[vk::PipelineShaderStageCreateInfo],
    vertex_input: &vk::PipelineVertexInputStateCreateInfo,
    input_assembly: &vk::PipelineInputAssemblyStateCreateInfo,
    viewport_state: &vk::PipelineViewportStateCreateInfo,
    rasterization: &vk::PipelineRasterizationStateCreateInfo,
    multisample: &vk::PipelineMultisampleStateCreateInfo,
    color_attachment: &vk::PipelineColorBlendAttachmentState,
    label: &str,
) -> Result<vk::Pipeline, String> {
    let color_attachments = [*color_attachment];
    let color_blend = vk::PipelineColorBlendStateCreateInfo::builder()
        .attachments(&color_attachments)
        .build();
    let color_formats = [target_format];
    let mut rendering = vk::PipelineRenderingCreateInfo::builder()
        .color_attachment_formats(&color_formats)
        .build();
    let mut flags = vk::PipelineCreateFlags2CreateInfo::builder()
        .flags(vk::PipelineCreateFlags2::DESCRIPTOR_HEAP_EXT)
        .build();
    let mut info = vk::GraphicsPipelineCreateInfo::builder()
        .stages(stages)
        .vertex_input_state(vertex_input)
        .input_assembly_state(input_assembly)
        .viewport_state(viewport_state)
        .rasterization_state(rasterization)
        .multisample_state(multisample)
        .color_blend_state(&color_blend)
        .layout(vk::PipelineLayout::null())
        .render_pass(vk::RenderPass::null())
        .subpass(0)
        .push_next(&mut rendering);
    info = info.push_next(&mut flags);
    let (pipelines, _) = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &[info.build()], None)
    }
    .map_err(|error| format!("vkCreateGraphicsPipelines(vulkanalia {label}): {error:?}"))?;
    Ok(pipelines[0])
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_destroy_decoded_image_present_pipeline_resources(
    device: &Device,
    resources: VulkanaliaDecodedImagePresentPipelineResources,
) {
    unsafe {
        device.destroy_pipeline(resources.scene_video_layer.pipeline, None);
        device.destroy_pipeline(resources.pipeline, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::native_vulkan::NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot;
    use crate::renderer::native_vulkan::native_vulkan_vulkanalia_descriptor_heap_image_sampler_plan;

    #[test]
    fn native_push_uses_descriptor_elements_and_retains_scene_extent() {
        let plan = native_vulkan_vulkanalia_descriptor_heap_image_sampler_plan(
            super::super::super::descriptor_heap::NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanInput {
                image_count: 2,
                properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
                    resource_heap_alignment: 64,
                    sampler_heap_alignment: 64,
                    max_resource_heap_size: 4096,
                    max_sampler_heap_size: 4096,
                    image_descriptor_size: 32,
                    sampler_descriptor_size: 16,
                    image_descriptor_alignment: 32,
                    sampler_descriptor_alignment: 16,
                    ..NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default()
                },
            },
        );
        let fullscreen = decoded_image_descriptor_push(
            &plan,
            DECODED_PRESENT_BINDINGS,
            DECODED_PRESENT_PUSH_BYTES,
            None,
        )
        .unwrap();
        let words = fullscreen
            .chunks_exact(4)
            .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(words, [0, 1, 0, 1]);

        let scene = decoded_image_descriptor_push(
            &plan,
            DECODED_SCENE_VIDEO_BINDINGS,
            DECODED_SCENE_VIDEO_PUSH_BYTES,
            Some(vk::Extent2D { width: 3840, height: 2160 }),
        )
        .unwrap();
        assert_eq!(f32::from_le_bytes(scene[0..4].try_into().unwrap()), 3840.0);
        assert_eq!(f32::from_le_bytes(scene[4..8].try_into().unwrap()), 2160.0);
        let words = scene[8..]
            .chunks_exact(4)
            .map(|word| u32::from_le_bytes(word.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(words, [0, 1, 0, 1]);
    }
}
