#![allow(dead_code)]

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use super::video_decode_submit::FFMPEG_VULKAN_DECODE_REFERENCE;
use super::video_session_images::VulkanaliaVideoSessionResourceImage;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaDecodedImagePresentSamplerSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub source_image_role: &'static str,
    pub picture_format: String,
    pub sampled_array_layer: u32,
    pub conversion_created: bool,
    pub sampler_created: bool,
    pub sampled_view_created: bool,
    pub descriptor_set_layout_created: bool,
    pub descriptor_pool_created: bool,
    pub descriptor_set_allocated: bool,
    pub descriptor_pool_combined_image_sampler_budget: u32,
    pub ycbcr_model: &'static str,
    pub ycbcr_range: &'static str,
    pub x_chroma_offset: &'static str,
    pub y_chroma_offset: &'static str,
    pub chroma_filter: &'static str,
    pub descriptor_type: &'static str,
    pub image_layout_for_shader: &'static str,
    pub present_pass_model: &'static str,
    pub queue_transfer_model: &'static str,
    pub uses_dynamic_rendering: bool,
    pub uses_synchronization2: bool,
    pub uses_submit2: bool,
    pub ffmpeg_reference: &'static str,
}

pub(super) struct VulkanaliaDecodedImagePresentSamplerResources {
    pub(super) conversion: vk::SamplerYcbcrConversion,
    pub(super) sampler: vk::Sampler,
    pub(super) sampled_view: vk::ImageView,
    pub(super) descriptor_set_layout: vk::DescriptorSetLayout,
    pub(super) descriptor_pool: vk::DescriptorPool,
    pub(super) descriptor_set: vk::DescriptorSet,
    pub(super) snapshot: NativeVulkanVulkanaliaDecodedImagePresentSamplerSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaDecodedImagePresentPipelineSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub target_format: String,
    pub extent: (u32, u32),
    pub shader_modules_created: bool,
    pub pipeline_layout_created: bool,
    pub pipeline_created: bool,
    pub render_pass_compatibility: &'static str,
    pub primitive_topology: &'static str,
    pub vertex_shader_model: &'static str,
    pub fragment_shader_model: &'static str,
    pub descriptor_sets: u32,
    pub uses_pipeline_rendering_create_info: bool,
    pub uses_dynamic_rendering: bool,
    pub uses_ycbcr_sampler_descriptor: bool,
    pub ffmpeg_reference: &'static str,
}

pub(super) struct VulkanaliaDecodedImagePresentPipelineResources {
    pub(super) pipeline_layout: vk::PipelineLayout,
    pub(super) pipeline: vk::Pipeline,
    pub(super) snapshot: NativeVulkanVulkanaliaDecodedImagePresentPipelineSnapshot,
}

pub(super) fn native_vulkan_vulkanalia_create_decoded_image_present_pipeline_resources(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
    descriptor_set_layout: vk::DescriptorSetLayout,
) -> Result<VulkanaliaDecodedImagePresentPipelineResources, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("decoded image present pipeline requires non-zero extent".to_owned());
    }

    let set_layouts = [descriptor_set_layout];
    let pipeline_layout_info = vk::PipelineLayoutCreateInfo::builder().set_layouts(&set_layouts);
    let pipeline_layout = unsafe { device.create_pipeline_layout(&pipeline_layout_info, None) }
        .map_err(|err| {
            format!("vkCreatePipelineLayout(vulkanalia decoded present dynamic rendering): {err:?}")
        })?;

    let result = (|| -> Result<VulkanaliaDecodedImagePresentPipelineResources, String> {
        let vertex_module = native_vulkan_vulkanalia_create_shader_module(
            device,
            &NATIVE_VULKAN_VULKANALIA_YCBCR_PRESENT_VERTEX_SPIRV,
            "decoded present vertex",
        )?;
        let result = (|| -> Result<VulkanaliaDecodedImagePresentPipelineResources, String> {
            let fragment_module = native_vulkan_vulkanalia_create_shader_module(
                device,
                &NATIVE_VULKAN_VULKANALIA_YCBCR_PRESENT_FRAGMENT_SPIRV,
                "decoded present fragment",
            )?;
            let result = (|| -> Result<VulkanaliaDecodedImagePresentPipelineResources, String> {
                let shader_entry = b"main\0";
                let stages = [
                    vk::PipelineShaderStageCreateInfo::builder()
                        .stage(vk::ShaderStageFlags::VERTEX)
                        .module(vertex_module)
                        .name(shader_entry)
                        .build(),
                    vk::PipelineShaderStageCreateInfo::builder()
                        .stage(vk::ShaderStageFlags::FRAGMENT)
                        .module(fragment_module)
                        .name(shader_entry)
                        .build(),
                ];
                let vertex_input = vk::PipelineVertexInputStateCreateInfo::builder().build();
                let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
                    .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
                    .build();
                let viewport = vk::Viewport::builder()
                    .x(0.0)
                    .y(0.0)
                    .width(extent.width as f32)
                    .height(extent.height as f32)
                    .min_depth(0.0)
                    .max_depth(1.0)
                    .build();
                let scissor = vk::Rect2D::builder()
                    .offset(vk::Offset2D { x: 0, y: 0 })
                    .extent(extent)
                    .build();
                let viewports = [viewport];
                let scissors = [scissor];
                let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
                    .viewports(&viewports)
                    .scissors(&scissors)
                    .build();
                let rasterization = vk::PipelineRasterizationStateCreateInfo::builder()
                    .polygon_mode(vk::PolygonMode::FILL)
                    .cull_mode(vk::CullModeFlags::NONE)
                    .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
                    .line_width(1.0)
                    .build();
                let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
                    .rasterization_samples(vk::SampleCountFlags::_1)
                    .build();
                let color_attachment = vk::PipelineColorBlendAttachmentState::builder()
                    .color_write_mask(
                        vk::ColorComponentFlags::R
                            | vk::ColorComponentFlags::G
                            | vk::ColorComponentFlags::B
                            | vk::ColorComponentFlags::A,
                    )
                    .blend_enable(false)
                    .build();
                let color_attachments = [color_attachment];
                let color_blend = vk::PipelineColorBlendStateCreateInfo::builder()
                    .attachments(&color_attachments)
                    .build();
                let color_attachment_formats = [target_format];
                let mut rendering_info = vk::PipelineRenderingCreateInfo::builder()
                    .color_attachment_formats(&color_attachment_formats)
                    .build();
                let pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
                    .stages(&stages)
                    .vertex_input_state(&vertex_input)
                    .input_assembly_state(&input_assembly)
                    .viewport_state(&viewport_state)
                    .rasterization_state(&rasterization)
                    .multisample_state(&multisample)
                    .color_blend_state(&color_blend)
                    .layout(pipeline_layout)
                    .render_pass(vk::RenderPass::null())
                    .subpass(0)
                    .push_next(&mut rendering_info)
                    .build();
                let (pipelines, _success_code) = unsafe {
                    device.create_graphics_pipelines(
                        vk::PipelineCache::null(),
                        &[pipeline_info],
                        None,
                    )
                }
                .map_err(|err| {
                    format!(
                        "vkCreateGraphicsPipelines(vulkanalia decoded present dynamic rendering): {err:?}"
                    )
                })?;
                let pipeline = pipelines[0];
                Ok(VulkanaliaDecodedImagePresentPipelineResources {
                    pipeline_layout,
                    pipeline,
                    snapshot: NativeVulkanVulkanaliaDecodedImagePresentPipelineSnapshot {
                        binding: "vulkanalia",
                        route: "decoded-image-dynamic-rendering-present-pipeline",
                        target_format: format!("{target_format:?}"),
                        extent: (extent.width, extent.height),
                        shader_modules_created: true,
                        pipeline_layout_created: true,
                        pipeline_created: true,
                        render_pass_compatibility: "dynamic-rendering-no-render-pass",
                        primitive_topology: "fullscreen-triangle",
                        vertex_shader_model: "gl_VertexIndex fullscreen triangle",
                        fragment_shader_model: "single combined YCbCr sampler2D",
                        descriptor_sets: 1,
                        uses_pipeline_rendering_create_info: true,
                        uses_dynamic_rendering: true,
                        uses_ycbcr_sampler_descriptor: true,
                        ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
                    },
                })
            })();
            unsafe {
                device.destroy_shader_module(fragment_module, None);
            }
            result
        })();
        unsafe {
            device.destroy_shader_module(vertex_module, None);
        }
        result
    })();

    if result.is_err() {
        unsafe {
            device.destroy_pipeline_layout(pipeline_layout, None);
        }
    }
    result
}

pub(super) fn native_vulkan_vulkanalia_destroy_decoded_image_present_pipeline_resources(
    device: &Device,
    resources: VulkanaliaDecodedImagePresentPipelineResources,
) {
    unsafe {
        device.destroy_pipeline(resources.pipeline, None);
        device.destroy_pipeline_layout(resources.pipeline_layout, None);
    }
}

pub(super) fn native_vulkan_vulkanalia_create_decoded_image_present_sampler_resources(
    device: &Device,
    resource_image: &VulkanaliaVideoSessionResourceImage,
    picture_format: vk::Format,
    sampled_array_layer: u32,
    video_queue_family_index: u32,
    present_queue_family_index: u32,
) -> Result<VulkanaliaDecodedImagePresentSamplerResources, String> {
    if !resource_image
        .snapshot
        .image_usage_flags
        .contains(&"sampled")
    {
        return Err("decoded image present sampler requires SAMPLED image usage".to_owned());
    }
    if sampled_array_layer >= resource_image.snapshot.array_layers {
        return Err(format!(
            "decoded image present sampler layer {sampled_array_layer} is outside {} image layers",
            resource_image.snapshot.array_layers
        ));
    }
    let ycbcr_model = native_vulkan_vulkanalia_decoded_image_ycbcr_model(picture_format)?;
    let ycbcr_range = vk::SamplerYcbcrRange::ITU_NARROW;
    let x_chroma_offset = vk::ChromaLocation::COSITED_EVEN;
    let y_chroma_offset = vk::ChromaLocation::MIDPOINT;
    let chroma_filter = vk::Filter::LINEAR;

    let conversion_info = vk::SamplerYcbcrConversionCreateInfo::builder()
        .format(picture_format)
        .ycbcr_model(ycbcr_model)
        .ycbcr_range(ycbcr_range)
        .components(vk::ComponentMapping::default())
        .x_chroma_offset(x_chroma_offset)
        .y_chroma_offset(y_chroma_offset)
        .chroma_filter(chroma_filter)
        .force_explicit_reconstruction(false);
    let conversion = unsafe { device.create_sampler_ycbcr_conversion(&conversion_info, None) }
        .map_err(|err| {
            format!("vkCreateSamplerYcbcrConversion(vulkanalia decoded present): {err:?}")
        })?;

    let result = (|| -> Result<VulkanaliaDecodedImagePresentSamplerResources, String> {
        let mut sampler_conversion_info = vk::SamplerYcbcrConversionInfo::builder()
            .conversion(conversion)
            .build();
        let sampler_info = vk::SamplerCreateInfo::builder()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .min_lod(0.0)
            .max_lod(0.0)
            .push_next(&mut sampler_conversion_info);
        let sampler = unsafe { device.create_sampler(&sampler_info, None) }
            .map_err(|err| format!("vkCreateSampler(vulkanalia decoded present ycbcr): {err:?}"))?;

        let result = (|| -> Result<VulkanaliaDecodedImagePresentSamplerResources, String> {
            let mut view_usage_info = vk::ImageViewUsageCreateInfo::builder()
                .usage(vk::ImageUsageFlags::SAMPLED)
                .build();
            let mut view_conversion_info = vk::SamplerYcbcrConversionInfo::builder()
                .conversion(conversion)
                .build();
            let subresource_range = vk::ImageSubresourceRange::builder()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(sampled_array_layer)
                .layer_count(1)
                .build();
            let sampled_view_info = vk::ImageViewCreateInfo::builder()
                .image(resource_image.image)
                .view_type(vk::ImageViewType::_2D)
                .format(picture_format)
                .subresource_range(subresource_range)
                .push_next(&mut view_usage_info)
                .push_next(&mut view_conversion_info);
            let sampled_view = unsafe { device.create_image_view(&sampled_view_info, None) }
                .map_err(|err| {
                    format!("vkCreateImageView(vulkanalia decoded present ycbcr): {err:?}")
                })?;

            let result = (|| -> Result<VulkanaliaDecodedImagePresentSamplerResources, String> {
                let immutable_samplers = [sampler];
                let descriptor_binding = vk::DescriptorSetLayoutBinding::builder()
                    .binding(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                    .immutable_samplers(&immutable_samplers)
                    .build();
                let descriptor_bindings = [descriptor_binding];
                let descriptor_set_layout_info =
                    vk::DescriptorSetLayoutCreateInfo::builder().bindings(&descriptor_bindings);
                let descriptor_set_layout = unsafe {
                    device.create_descriptor_set_layout(&descriptor_set_layout_info, None)
                }
                .map_err(|err| {
                    format!(
                        "vkCreateDescriptorSetLayout(vulkanalia decoded present ycbcr): {err:?}"
                    )
                })?;

                let result =
                    (|| -> Result<VulkanaliaDecodedImagePresentSamplerResources, String> {
                        let descriptor_budget =
                            NATIVE_VULKAN_VULKANALIA_YCBCR_DESCRIPTOR_POOL_BUDGET;
                        let pool_size = vk::DescriptorPoolSize::builder()
                            .type_(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                            .descriptor_count(descriptor_budget)
                            .build();
                        let pool_sizes = [pool_size];
                        let descriptor_pool_info = vk::DescriptorPoolCreateInfo::builder()
                            .max_sets(1)
                            .pool_sizes(&pool_sizes);
                        let descriptor_pool = unsafe {
                            device.create_descriptor_pool(&descriptor_pool_info, None)
                        }
                        .map_err(|err| {
                            format!(
                                "vkCreateDescriptorPool(vulkanalia decoded present ycbcr): {err:?}"
                            )
                        })?;

                        let result =
                        (|| -> Result<VulkanaliaDecodedImagePresentSamplerResources, String> {
                            let descriptor_set_layouts = [descriptor_set_layout];
                            let allocate_info = vk::DescriptorSetAllocateInfo::builder()
                                .descriptor_pool(descriptor_pool)
                                .set_layouts(&descriptor_set_layouts);
                            let descriptor_sets =
                                unsafe { device.allocate_descriptor_sets(&allocate_info) }
                                    .map_err(|err| {
                                        format!(
                                            "vkAllocateDescriptorSets(vulkanalia decoded present ycbcr): {err:?}"
                                        )
                                    })?;
                            let descriptor_set = descriptor_sets[0];
                            let descriptor_image = vk::DescriptorImageInfo::builder()
                                .sampler(sampler)
                                .image_view(sampled_view)
                                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                                .build();
                            let descriptor_images = [descriptor_image];
                            let write = vk::WriteDescriptorSet::builder()
                                .dst_set(descriptor_set)
                                .dst_binding(0)
                                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                                .image_info(&descriptor_images)
                                .build();
                            let descriptor_copies: [vk::CopyDescriptorSet; 0] = [];
                            unsafe {
                                device.update_descriptor_sets(&[write], &descriptor_copies);
                            }

                            Ok(VulkanaliaDecodedImagePresentSamplerResources {
                                conversion,
                                sampler,
                                sampled_view,
                                descriptor_set_layout,
                                descriptor_pool,
                                descriptor_set,
                                snapshot: NativeVulkanVulkanaliaDecodedImagePresentSamplerSnapshot {
                                    binding: "vulkanalia",
                                    route: "decoded-image-ycbcr-sampler-present-resource",
                                    source_image_role: resource_image.snapshot.role,
                                    picture_format: format!("{picture_format:?}"),
                                    sampled_array_layer,
                                    conversion_created: true,
                                    sampler_created: true,
                                    sampled_view_created: true,
                                    descriptor_set_layout_created: true,
                                    descriptor_pool_created: true,
                                    descriptor_set_allocated: true,
                                    descriptor_pool_combined_image_sampler_budget:
                                        descriptor_budget,
                                    ycbcr_model: sampler_ycbcr_model_label(ycbcr_model),
                                    ycbcr_range: sampler_ycbcr_range_label(ycbcr_range),
                                    x_chroma_offset: chroma_location_label(x_chroma_offset),
                                    y_chroma_offset: chroma_location_label(y_chroma_offset),
                                    chroma_filter: filter_label(chroma_filter),
                                    descriptor_type: "combined-image-sampler",
                                    image_layout_for_shader: "shader-read-only-optimal",
                                    present_pass_model:
                                        "decoded image -> immutable YCbCr combined image sampler -> dynamic rendering fullscreen graphics pass -> swapchain",
                                    queue_transfer_model:
                                        native_vulkan_vulkanalia_decoded_image_present_queue_model(
                                            video_queue_family_index,
                                            present_queue_family_index,
                                        ),
                                    uses_dynamic_rendering: true,
                                    uses_synchronization2: true,
                                    uses_submit2: true,
                                    ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
                                },
                            })
                        })();

                        if result.is_err() {
                            unsafe {
                                device.destroy_descriptor_pool(descriptor_pool, None);
                            }
                        }
                        result
                    })();

                if result.is_err() {
                    unsafe {
                        device.destroy_descriptor_set_layout(descriptor_set_layout, None);
                    }
                }
                result
            })();

            if result.is_err() {
                unsafe {
                    device.destroy_image_view(sampled_view, None);
                }
            }
            result
        })();

        if result.is_err() {
            unsafe {
                device.destroy_sampler(sampler, None);
            }
        }
        result
    })();

    if result.is_err() {
        unsafe {
            device.destroy_sampler_ycbcr_conversion(conversion, None);
        }
    }
    result
}

pub(super) fn native_vulkan_vulkanalia_destroy_decoded_image_present_sampler_resources(
    device: &Device,
    resources: VulkanaliaDecodedImagePresentSamplerResources,
) {
    unsafe {
        device.destroy_descriptor_pool(resources.descriptor_pool, None);
        device.destroy_descriptor_set_layout(resources.descriptor_set_layout, None);
        device.destroy_image_view(resources.sampled_view, None);
        device.destroy_sampler(resources.sampler, None);
        device.destroy_sampler_ycbcr_conversion(resources.conversion, None);
    }
}

pub(super) fn native_vulkan_vulkanalia_decoded_image_present_command_order(
    same_queue_family: bool,
) -> &'static [&'static str] {
    if same_queue_family {
        &[
            "queue_submit2_decode",
            "cmd_pipeline_barrier2_shader_read",
            "cmd_begin_rendering",
            "cmd_bind_ycbcr_descriptor",
            "cmd_draw_fullscreen_triangle",
            "cmd_end_rendering",
            "cmd_pipeline_barrier2_present",
            "queue_submit2_present",
            "queue_present_khr",
        ]
    } else {
        &[
            "queue_submit2_decode",
            "cmd_pipeline_barrier2_video_release",
            "cmd_pipeline_barrier2_graphics_acquire_shader_read",
            "cmd_begin_rendering",
            "cmd_bind_ycbcr_descriptor",
            "cmd_draw_fullscreen_triangle",
            "cmd_end_rendering",
            "cmd_pipeline_barrier2_present",
            "queue_submit2_present",
            "queue_present_khr",
        ]
    }
}

const NATIVE_VULKAN_VULKANALIA_YCBCR_DESCRIPTOR_POOL_BUDGET: u32 = 4;

fn native_vulkan_vulkanalia_decoded_image_ycbcr_model(
    picture_format: vk::Format,
) -> Result<vk::SamplerYcbcrModelConversion, String> {
    match picture_format {
        vk::Format::G8_B8R8_2PLANE_420_UNORM
        | vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16 => {
            Ok(vk::SamplerYcbcrModelConversion::YCBCR_709)
        }
        _ => Err(format!(
            "{picture_format:?} is not a retained decoded NV12/P010 YCbCr format"
        )),
    }
}

fn native_vulkan_vulkanalia_decoded_image_present_queue_model(
    video_queue_family_index: u32,
    present_queue_family_index: u32,
) -> &'static str {
    if video_queue_family_index == present_queue_family_index {
        "single queue family: decode submission orders shader-read barrier before dynamic rendering"
    } else {
        "dedicated video queue releases decoded image, graphics/present queue acquires it with sync2 before dynamic rendering"
    }
}

fn native_vulkan_vulkanalia_create_shader_module(
    device: &Device,
    code: &[u32],
    label: &'static str,
) -> Result<vk::ShaderModule, String> {
    if code.first().copied() != Some(0x0723_0203) {
        return Err(format!(
            "decoded present {label} shader is not valid SPIR-V bytecode"
        ));
    }
    let create_info = vk::ShaderModuleCreateInfo::builder()
        .code(code)
        .code_size(native_vulkan_vulkanalia_shader_code_size_bytes(code));
    unsafe { device.create_shader_module(&create_info, None) }
        .map_err(|err| format!("vkCreateShaderModule(vulkanalia {label}): {err:?}"))
}

fn native_vulkan_vulkanalia_shader_code_size_bytes(code: &[u32]) -> usize {
    std::mem::size_of_val(code)
}

fn sampler_ycbcr_model_label(model: vk::SamplerYcbcrModelConversion) -> &'static str {
    match model {
        vk::SamplerYcbcrModelConversion::YCBCR_709 => "ycbcr-709",
        vk::SamplerYcbcrModelConversion::YCBCR_601 => "ycbcr-601",
        vk::SamplerYcbcrModelConversion::YCBCR_2020 => "ycbcr-2020",
        vk::SamplerYcbcrModelConversion::YCBCR_IDENTITY => "ycbcr-identity",
        vk::SamplerYcbcrModelConversion::RGB_IDENTITY => "rgb-identity",
        _ => "unknown",
    }
}

fn sampler_ycbcr_range_label(range: vk::SamplerYcbcrRange) -> &'static str {
    match range {
        vk::SamplerYcbcrRange::ITU_FULL => "itu-full",
        vk::SamplerYcbcrRange::ITU_NARROW => "itu-narrow",
        _ => "unknown",
    }
}

fn chroma_location_label(location: vk::ChromaLocation) -> &'static str {
    match location {
        vk::ChromaLocation::COSITED_EVEN => "cosited-even",
        vk::ChromaLocation::MIDPOINT => "midpoint",
        _ => "unknown",
    }
}

fn filter_label(filter: vk::Filter) -> &'static str {
    match filter {
        vk::Filter::NEAREST => "nearest",
        vk::Filter::LINEAR => "linear",
        _ => "unknown",
    }
}

const NATIVE_VULKAN_VULKANALIA_YCBCR_PRESENT_VERTEX_SPIRV: [u32; 357] = [
    119734787, 65536, 851979, 51, 0, 131089, 1, 393227, 1, 1280527431, 1685353262, 808793134, 0,
    196622, 0, 1, 524303, 0, 4, 1852399981, 0, 32, 36, 47, 196611, 2, 450, 655364, 1197427783,
    1279741775, 1885560645, 1953718128, 1600482425, 1701734764, 1919509599, 1769235301, 25974,
    524292, 1197427783, 1279741775, 1852399429, 1685417059, 1768185701, 1952671090, 6649449,
    262149, 4, 1852399981, 0, 196613, 12, 7565168, 196613, 19, 30325, 393221, 30, 1348430951,
    1700164197, 2019914866, 0, 393222, 30, 0, 1348430951, 1953067887, 7237481, 458758, 30, 1,
    1348430951, 1953393007, 1702521171, 0, 458758, 30, 2, 1130327143, 1148217708, 1635021673,
    6644590, 458758, 30, 3, 1130327143, 1147956341, 1635021673, 6644590, 196613, 32, 0, 393221, 36,
    1449094247, 1702130277, 1684949368, 30821, 262149, 47, 1987403638, 0, 196679, 30, 2, 327752,
    30, 0, 11, 0, 327752, 30, 1, 11, 1, 327752, 30, 2, 11, 3, 327752, 30, 3, 11, 4, 262215, 36, 11,
    42, 262215, 47, 30, 0, 131091, 2, 196641, 3, 2, 196630, 6, 32, 262167, 7, 6, 2, 262165, 8, 32,
    0, 262187, 8, 9, 3, 262172, 10, 7, 9, 262176, 11, 7, 10, 262187, 6, 13, 3212836864, 327724, 7,
    14, 13, 13, 262187, 6, 15, 1077936128, 327724, 7, 16, 15, 13, 327724, 7, 17, 13, 15, 393260,
    10, 18, 14, 16, 17, 262187, 6, 20, 0, 262187, 6, 21, 1065353216, 327724, 7, 22, 20, 21, 262187,
    6, 23, 1073741824, 327724, 7, 24, 23, 21, 327724, 7, 25, 20, 13, 393260, 10, 26, 22, 24, 25,
    262167, 27, 6, 4, 262187, 8, 28, 1, 262172, 29, 6, 28, 393246, 30, 27, 6, 29, 29, 262176, 31,
    3, 30, 262203, 31, 32, 3, 262165, 33, 32, 1, 262187, 33, 34, 0, 262176, 35, 1, 33, 262203, 35,
    36, 1, 262176, 38, 7, 7, 262176, 44, 3, 27, 262176, 46, 3, 7, 262203, 46, 47, 3, 327734, 2, 4,
    0, 3, 131320, 5, 262203, 11, 12, 7, 262203, 11, 19, 7, 196670, 12, 18, 196670, 19, 26, 262205,
    33, 37, 36, 327745, 38, 39, 12, 37, 262205, 7, 40, 39, 327761, 6, 41, 40, 0, 327761, 6, 42, 40,
    1, 458832, 27, 43, 41, 42, 20, 21, 327745, 44, 45, 32, 34, 196670, 45, 43, 262205, 33, 48, 36,
    327745, 38, 49, 19, 48, 262205, 7, 50, 49, 196670, 47, 50, 65789, 65592,
];

const NATIVE_VULKAN_VULKANALIA_YCBCR_PRESENT_FRAGMENT_SPIRV: [u32; 157] = [
    119734787, 65536, 851979, 20, 0, 131089, 1, 393227, 1, 1280527431, 1685353262, 808793134, 0,
    196622, 0, 1, 458767, 4, 4, 1852399981, 0, 9, 17, 196624, 4, 7, 196611, 2, 450, 655364,
    1197427783, 1279741775, 1885560645, 1953718128, 1600482425, 1701734764, 1919509599, 1769235301,
    25974, 524292, 1197427783, 1279741775, 1852399429, 1685417059, 1768185701, 1952671090, 6649449,
    262149, 4, 1852399981, 0, 327685, 9, 1601467759, 1869377379, 114, 262149, 13, 1769365365,
    7300452, 262149, 17, 1987403638, 0, 262215, 9, 30, 0, 262215, 13, 33, 0, 262215, 13, 34, 0,
    262215, 17, 30, 0, 131091, 2, 196641, 3, 2, 196630, 6, 32, 262167, 7, 6, 4, 262176, 8, 3, 7,
    262203, 8, 9, 3, 589849, 10, 6, 1, 0, 0, 0, 1, 0, 196635, 11, 10, 262176, 12, 0, 11, 262203,
    12, 13, 0, 262167, 15, 6, 2, 262176, 16, 1, 15, 262203, 16, 17, 1, 327734, 2, 4, 0, 3, 131320,
    5, 262205, 11, 14, 13, 262205, 15, 18, 17, 327767, 7, 19, 14, 18, 196670, 9, 19, 65789, 65592,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoded_image_present_order_keeps_queue_ownership_explicit() {
        let split = native_vulkan_vulkanalia_decoded_image_present_command_order(false);
        assert!(split.contains(&"cmd_pipeline_barrier2_video_release"));
        assert!(split.contains(&"cmd_pipeline_barrier2_graphics_acquire_shader_read"));
        assert!(split.contains(&"cmd_begin_rendering"));
        assert!(split.contains(&"queue_submit2_present"));

        let same = native_vulkan_vulkanalia_decoded_image_present_command_order(true);
        assert!(!same.contains(&"cmd_pipeline_barrier2_video_release"));
        assert!(same.contains(&"cmd_bind_ycbcr_descriptor"));
    }

    #[test]
    fn decoded_image_present_ycbcr_model_accepts_current_video_formats() {
        assert_eq!(
            native_vulkan_vulkanalia_decoded_image_ycbcr_model(
                vk::Format::G8_B8R8_2PLANE_420_UNORM,
            )
            .unwrap(),
            vk::SamplerYcbcrModelConversion::YCBCR_709
        );
        assert_eq!(
            native_vulkan_vulkanalia_decoded_image_ycbcr_model(
                vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16,
            )
            .unwrap(),
            vk::SamplerYcbcrModelConversion::YCBCR_709
        );
        assert!(
            native_vulkan_vulkanalia_decoded_image_ycbcr_model(vk::Format::R8G8B8A8_UNORM).is_err()
        );
    }

    #[test]
    fn shader_module_code_size_uses_bytes_not_words() {
        assert_eq!(
            native_vulkan_vulkanalia_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_YCBCR_PRESENT_VERTEX_SPIRV
            ),
            NATIVE_VULKAN_VULKANALIA_YCBCR_PRESENT_VERTEX_SPIRV.len() * 4
        );
    }
}
