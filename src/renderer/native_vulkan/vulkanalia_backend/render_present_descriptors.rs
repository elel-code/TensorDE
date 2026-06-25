use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use super::descriptor_heap::{
    NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanInput,
    NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    VulkanaliaDescriptorHeapImageSamplerResources,
    native_vulkan_vulkanalia_create_descriptor_heap_image_sampler_resources,
    native_vulkan_vulkanalia_descriptor_heap_image_sampler_plan,
    native_vulkan_vulkanalia_destroy_descriptor_heap_image_sampler_resources,
    native_vulkan_vulkanalia_write_descriptor_heap_image_sampler,
};
use super::features::NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot;
use super::video_decode_submit::FFMPEG_VULKAN_DECODE_REFERENCE;
use super::video_session_images::VulkanaliaVideoSessionResourceImage;

const NATIVE_VULKAN_VULKANALIA_YCBCR_DESCRIPTOR_POOL_BUDGET: u32 = 4;

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
    pub descriptor_heap_available: bool,
    pub descriptor_heap_plan_ready: bool,
    pub descriptor_heap_resources_created: bool,
    pub descriptor_heap_resource_descriptor_written: bool,
    pub descriptor_heap_sampler_descriptor_written: bool,
    pub descriptor_heap_plan: NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
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
    pub(super) descriptor_heap: Option<VulkanaliaDescriptorHeapImageSamplerResources>,
    pub(super) snapshot: NativeVulkanVulkanaliaDecodedImagePresentSamplerSnapshot,
}

struct VulkanaliaTraditionalDecodedImagePresentDescriptorResources {
    descriptor_pool: vk::DescriptorPool,
    descriptor_set: vk::DescriptorSet,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn native_vulkan_vulkanalia_create_decoded_image_present_sampler_resources(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    resource_image: &VulkanaliaVideoSessionResourceImage,
    picture_format: vk::Format,
    sampled_array_layer: u32,
    video_queue_family_index: u32,
    present_queue_family_index: u32,
    descriptor_heap_enabled: bool,
    descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
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
    let descriptor_heap_plan = native_vulkan_vulkanalia_descriptor_heap_image_sampler_plan(
        NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanInput {
            image_count: 1,
            properties: descriptor_heap_properties,
        },
    );

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

    let mut sampler = vk::Sampler::null();
    let mut sampled_view = vk::ImageView::null();
    let mut descriptor_set_layout = vk::DescriptorSetLayout::null();
    let mut descriptor_pool = vk::DescriptorPool::null();
    let mut descriptor_set = vk::DescriptorSet::null();
    let mut descriptor_heap: Option<VulkanaliaDescriptorHeapImageSamplerResources> = None;

    let result = (|| -> Result<VulkanaliaDecodedImagePresentSamplerResources, String> {
        let mut sampler_conversion_info = vk::SamplerYcbcrConversionInfo::builder()
            .conversion(conversion)
            .build();
        let sampler_info = native_vulkan_vulkanalia_decoded_image_sampler_create_info(
            &mut sampler_conversion_info,
        );
        sampler = unsafe { device.create_sampler(&sampler_info, None) }
            .map_err(|err| format!("vkCreateSampler(vulkanalia decoded present ycbcr): {err:?}"))?;

        let mut view_usage_info = vk::ImageViewUsageCreateInfo::builder()
            .usage(vk::ImageUsageFlags::SAMPLED)
            .build();
        let mut view_conversion_info = vk::SamplerYcbcrConversionInfo::builder()
            .conversion(conversion)
            .build();
        sampled_view = native_vulkan_vulkanalia_create_decoded_image_sampled_view(
            device,
            resource_image.image,
            picture_format,
            sampled_array_layer,
            &mut view_usage_info,
            &mut view_conversion_info,
        )?;

        descriptor_set_layout =
            native_vulkan_vulkanalia_create_decoded_image_present_descriptor_set_layout(
                device, sampler,
            )?;

        if descriptor_heap_enabled && descriptor_heap_plan.backend_ready {
            descriptor_heap = Some(
                native_vulkan_vulkanalia_create_decoded_image_present_descriptor_heap(
                    device,
                    memory_properties,
                    resource_image.image,
                    picture_format,
                    sampled_array_layer,
                    conversion,
                    &sampler_info,
                    &descriptor_heap_plan,
                )?,
            );
        }

        let mut descriptor_pool_created = false;
        let mut descriptor_set_allocated = false;
        if descriptor_heap.is_none() {
            let traditional =
                native_vulkan_vulkanalia_create_traditional_decoded_image_present_descriptors(
                    device,
                    descriptor_set_layout,
                    sampler,
                    sampled_view,
                )?;
            descriptor_pool = traditional.descriptor_pool;
            descriptor_set = traditional.descriptor_set;
            descriptor_pool_created = true;
            descriptor_set_allocated = true;
        }

        let descriptor_heap_resources_created = descriptor_heap.is_some();
        let descriptor_heap_resource_descriptor_written = descriptor_heap
            .as_ref()
            .map(|heap| heap.snapshot.resource_descriptor_written)
            .unwrap_or(false);
        let descriptor_heap_sampler_descriptor_written = descriptor_heap
            .as_ref()
            .map(|heap| heap.snapshot.sampler_descriptor_written)
            .unwrap_or(false);

        Ok(VulkanaliaDecodedImagePresentSamplerResources {
            conversion,
            sampler,
            sampled_view,
            descriptor_set_layout,
            descriptor_pool,
            descriptor_set,
            descriptor_heap: descriptor_heap.take(),
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
                descriptor_pool_created,
                descriptor_set_allocated,
                descriptor_pool_combined_image_sampler_budget: if descriptor_pool_created {
                    NATIVE_VULKAN_VULKANALIA_YCBCR_DESCRIPTOR_POOL_BUDGET
                } else {
                    0
                },
                descriptor_heap_available: descriptor_heap_enabled,
                descriptor_heap_plan_ready: descriptor_heap_enabled
                    && descriptor_heap_plan.backend_ready,
                descriptor_heap_resources_created,
                descriptor_heap_resource_descriptor_written,
                descriptor_heap_sampler_descriptor_written,
                descriptor_heap_plan,
                ycbcr_model: sampler_ycbcr_model_label(ycbcr_model),
                ycbcr_range: sampler_ycbcr_range_label(ycbcr_range),
                x_chroma_offset: chroma_location_label(x_chroma_offset),
                y_chroma_offset: chroma_location_label(y_chroma_offset),
                chroma_filter: filter_label(chroma_filter),
                descriptor_type: "combined-image-sampler",
                image_layout_for_shader: "shader-read-only-optimal",
                present_pass_model: if descriptor_heap_resources_created {
                    "decoded image -> VK_EXT_descriptor_heap YCbCr sampler mapping -> dynamic rendering fullscreen graphics pass -> swapchain"
                } else {
                    "decoded image -> immutable YCbCr combined image sampler -> dynamic rendering fullscreen graphics pass -> swapchain"
                },
                queue_transfer_model: native_vulkan_vulkanalia_decoded_image_present_queue_model(
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
        if let Some(descriptor_heap) = descriptor_heap.take() {
            native_vulkan_vulkanalia_destroy_descriptor_heap_image_sampler_resources(
                device,
                descriptor_heap,
            );
        }
        unsafe {
            if descriptor_pool != vk::DescriptorPool::null() {
                device.destroy_descriptor_pool(descriptor_pool, None);
            }
            if descriptor_set_layout != vk::DescriptorSetLayout::null() {
                device.destroy_descriptor_set_layout(descriptor_set_layout, None);
            }
            if sampled_view != vk::ImageView::null() {
                device.destroy_image_view(sampled_view, None);
            }
            if sampler != vk::Sampler::null() {
                device.destroy_sampler(sampler, None);
            }
            device.destroy_sampler_ycbcr_conversion(conversion, None);
        }
    }

    result
}

pub(super) fn native_vulkan_vulkanalia_retarget_decoded_image_present_sampler_layer(
    device: &Device,
    resource_image: &VulkanaliaVideoSessionResourceImage,
    picture_format: vk::Format,
    resources: &mut VulkanaliaDecodedImagePresentSamplerResources,
    sampled_array_layer: u32,
) -> Result<(), String> {
    if sampled_array_layer >= resource_image.snapshot.array_layers {
        return Err(format!(
            "decoded image present sampler layer {sampled_array_layer} is outside {} image layers",
            resource_image.snapshot.array_layers
        ));
    }
    if sampled_array_layer == resources.snapshot.sampled_array_layer {
        return Ok(());
    }

    let mut view_usage_info = vk::ImageViewUsageCreateInfo::builder()
        .usage(vk::ImageUsageFlags::SAMPLED)
        .build();
    let mut view_conversion_info = vk::SamplerYcbcrConversionInfo::builder()
        .conversion(resources.conversion)
        .build();
    let sampled_view = native_vulkan_vulkanalia_create_decoded_image_sampled_view(
        device,
        resource_image.image,
        picture_format,
        sampled_array_layer,
        &mut view_usage_info,
        &mut view_conversion_info,
    )?;

    if let Some(descriptor_heap) = resources.descriptor_heap.as_mut() {
        let mut heap_view_usage_info = vk::ImageViewUsageCreateInfo::builder()
            .usage(vk::ImageUsageFlags::SAMPLED)
            .build();
        let mut heap_view_conversion_info = vk::SamplerYcbcrConversionInfo::builder()
            .conversion(resources.conversion)
            .build();
        let heap_sampled_view_info =
            native_vulkan_vulkanalia_decoded_image_sampled_view_create_info(
                resource_image.image,
                picture_format,
                sampled_array_layer,
                &mut heap_view_usage_info,
                &mut heap_view_conversion_info,
            );
        let mut sampler_conversion_info = vk::SamplerYcbcrConversionInfo::builder()
            .conversion(resources.conversion)
            .build();
        let sampler_info = native_vulkan_vulkanalia_decoded_image_sampler_create_info(
            &mut sampler_conversion_info,
        );
        if let Err(err) = native_vulkan_vulkanalia_write_descriptor_heap_image_sampler(
            device,
            descriptor_heap,
            0,
            &heap_sampled_view_info,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            &sampler_info,
        ) {
            unsafe {
                device.destroy_image_view(sampled_view, None);
            }
            return Err(err);
        }
        resources
            .snapshot
            .descriptor_heap_resource_descriptor_written =
            descriptor_heap.snapshot.resource_descriptor_written;
        resources
            .snapshot
            .descriptor_heap_sampler_descriptor_written =
            descriptor_heap.snapshot.sampler_descriptor_written;
    }

    if resources.descriptor_heap.is_some() {
        unsafe {
            device.destroy_image_view(resources.sampled_view, None);
        }
        resources.sampled_view = sampled_view;
        resources.snapshot.sampled_array_layer = sampled_array_layer;
        return Ok(());
    }

    native_vulkan_vulkanalia_update_traditional_decoded_image_present_descriptor_set(
        device,
        resources.descriptor_set,
        resources.sampler,
        sampled_view,
    );
    unsafe {
        device.destroy_image_view(resources.sampled_view, None);
    }

    resources.sampled_view = sampled_view;
    resources.snapshot.sampled_array_layer = sampled_array_layer;
    Ok(())
}

pub(super) fn native_vulkan_vulkanalia_destroy_decoded_image_present_sampler_resources(
    device: &Device,
    resources: VulkanaliaDecodedImagePresentSamplerResources,
) {
    unsafe {
        if let Some(descriptor_heap) = resources.descriptor_heap {
            native_vulkan_vulkanalia_destroy_descriptor_heap_image_sampler_resources(
                device,
                descriptor_heap,
            );
        }
        if resources.descriptor_pool != vk::DescriptorPool::null() {
            device.destroy_descriptor_pool(resources.descriptor_pool, None);
        }
        device.destroy_descriptor_set_layout(resources.descriptor_set_layout, None);
        device.destroy_image_view(resources.sampled_view, None);
        device.destroy_sampler(resources.sampler, None);
        device.destroy_sampler_ycbcr_conversion(resources.conversion, None);
    }
}

fn native_vulkan_vulkanalia_create_decoded_image_present_descriptor_heap(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    image: vk::Image,
    picture_format: vk::Format,
    sampled_array_layer: u32,
    conversion: vk::SamplerYcbcrConversion,
    sampler_info: &vk::SamplerCreateInfo,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
) -> Result<VulkanaliaDescriptorHeapImageSamplerResources, String> {
    let mut heap_view_usage_info = vk::ImageViewUsageCreateInfo::builder()
        .usage(vk::ImageUsageFlags::SAMPLED)
        .build();
    let mut heap_view_conversion_info = vk::SamplerYcbcrConversionInfo::builder()
        .conversion(conversion)
        .build();
    let heap_sampled_view_info = native_vulkan_vulkanalia_decoded_image_sampled_view_create_info(
        image,
        picture_format,
        sampled_array_layer,
        &mut heap_view_usage_info,
        &mut heap_view_conversion_info,
    );
    let mut heap_resources =
        native_vulkan_vulkanalia_create_descriptor_heap_image_sampler_resources(
            device,
            memory_properties,
            descriptor_heap_plan,
        )?;
    if let Err(err) = native_vulkan_vulkanalia_write_descriptor_heap_image_sampler(
        device,
        &mut heap_resources,
        0,
        &heap_sampled_view_info,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        sampler_info,
    ) {
        native_vulkan_vulkanalia_destroy_descriptor_heap_image_sampler_resources(
            device,
            heap_resources,
        );
        return Err(err);
    }
    Ok(heap_resources)
}

fn native_vulkan_vulkanalia_create_decoded_image_present_descriptor_set_layout(
    device: &Device,
    sampler: vk::Sampler,
) -> Result<vk::DescriptorSetLayout, String> {
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
    unsafe { device.create_descriptor_set_layout(&descriptor_set_layout_info, None) }.map_err(
        |err| format!("vkCreateDescriptorSetLayout(vulkanalia decoded present ycbcr): {err:?}"),
    )
}

fn native_vulkan_vulkanalia_create_traditional_decoded_image_present_descriptors(
    device: &Device,
    descriptor_set_layout: vk::DescriptorSetLayout,
    sampler: vk::Sampler,
    sampled_view: vk::ImageView,
) -> Result<VulkanaliaTraditionalDecodedImagePresentDescriptorResources, String> {
    let pool_size = vk::DescriptorPoolSize::builder()
        .type_(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .descriptor_count(NATIVE_VULKAN_VULKANALIA_YCBCR_DESCRIPTOR_POOL_BUDGET)
        .build();
    let pool_sizes = [pool_size];
    let descriptor_pool_info = vk::DescriptorPoolCreateInfo::builder()
        .max_sets(1)
        .pool_sizes(&pool_sizes);
    let descriptor_pool = unsafe { device.create_descriptor_pool(&descriptor_pool_info, None) }
        .map_err(|err| {
            format!("vkCreateDescriptorPool(vulkanalia decoded present ycbcr): {err:?}")
        })?;

    let result =
        (|| -> Result<VulkanaliaTraditionalDecodedImagePresentDescriptorResources, String> {
            let descriptor_set_layouts = [descriptor_set_layout];
            let allocate_info = vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&descriptor_set_layouts);
            let descriptor_sets = unsafe { device.allocate_descriptor_sets(&allocate_info) }
                .map_err(|err| {
                    format!("vkAllocateDescriptorSets(vulkanalia decoded present ycbcr): {err:?}")
                })?;
            let descriptor_set = descriptor_sets[0];
            native_vulkan_vulkanalia_update_traditional_decoded_image_present_descriptor_set(
                device,
                descriptor_set,
                sampler,
                sampled_view,
            );
            Ok(
                VulkanaliaTraditionalDecodedImagePresentDescriptorResources {
                    descriptor_pool,
                    descriptor_set,
                },
            )
        })();

    if result.is_err() {
        unsafe {
            device.destroy_descriptor_pool(descriptor_pool, None);
        }
    }
    result
}

fn native_vulkan_vulkanalia_update_traditional_decoded_image_present_descriptor_set(
    device: &Device,
    descriptor_set: vk::DescriptorSet,
    sampler: vk::Sampler,
    sampled_view: vk::ImageView,
) {
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
}

pub(super) fn native_vulkan_vulkanalia_decoded_image_sampler_create_info(
    sampler_conversion_info: &mut vk::SamplerYcbcrConversionInfo,
) -> vk::SamplerCreateInfo {
    vk::SamplerCreateInfo::builder()
        .mag_filter(vk::Filter::LINEAR)
        .min_filter(vk::Filter::LINEAR)
        .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .min_lod(0.0)
        .max_lod(0.0)
        .push_next(sampler_conversion_info)
        .build()
}

fn native_vulkan_vulkanalia_create_decoded_image_sampled_view(
    device: &Device,
    image: vk::Image,
    picture_format: vk::Format,
    sampled_array_layer: u32,
    view_usage_info: &mut vk::ImageViewUsageCreateInfo,
    view_conversion_info: &mut vk::SamplerYcbcrConversionInfo,
) -> Result<vk::ImageView, String> {
    let sampled_view_info = native_vulkan_vulkanalia_decoded_image_sampled_view_create_info(
        image,
        picture_format,
        sampled_array_layer,
        view_usage_info,
        view_conversion_info,
    );
    unsafe { device.create_image_view(&sampled_view_info, None) }
        .map_err(|err| format!("vkCreateImageView(vulkanalia decoded present ycbcr): {err:?}"))
}

fn native_vulkan_vulkanalia_decoded_image_sampled_view_create_info<'a>(
    image: vk::Image,
    picture_format: vk::Format,
    sampled_array_layer: u32,
    view_usage_info: &'a mut vk::ImageViewUsageCreateInfo,
    view_conversion_info: &'a mut vk::SamplerYcbcrConversionInfo,
) -> vk::ImageViewCreateInfo {
    let subresource_range = vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(sampled_array_layer)
        .layer_count(1)
        .build();
    vk::ImageViewCreateInfo::builder()
        .image(image)
        .view_type(vk::ImageViewType::_2D)
        .format(picture_format)
        .subresource_range(subresource_range)
        .push_next(view_usage_info)
        .push_next(view_conversion_info)
        .build()
}

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
