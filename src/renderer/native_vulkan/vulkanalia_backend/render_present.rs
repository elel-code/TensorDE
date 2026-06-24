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
}
