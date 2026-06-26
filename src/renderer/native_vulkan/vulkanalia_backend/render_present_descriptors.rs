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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaDecodedImagePresentSamplerSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub source_image_role: &'static str,
    pub picture_format: String,
    pub sampled_array_layer: u32,
    pub y_plane_format: String,
    pub uv_plane_format: String,
    pub plane_descriptor_count: u32,
    pub descriptor_heap_only: bool,
    pub descriptor_heap_available: bool,
    pub descriptor_heap_plan_ready: bool,
    pub descriptor_heap_resources_created: bool,
    pub descriptor_heap_resource_descriptor_written: bool,
    pub descriptor_heap_sampler_descriptor_written: bool,
    pub descriptor_heap_plan: NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
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
    pub(super) descriptor_heap: VulkanaliaDescriptorHeapImageSamplerResources,
    pub(super) snapshot: NativeVulkanVulkanaliaDecodedImagePresentSamplerSnapshot,
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
    if !resource_image
        .snapshot
        .image_create_flags
        .contains(&"mutable-format")
    {
        return Err(
            "decoded image present plane sampling requires a mutable-format decoded image"
                .to_owned(),
        );
    }
    if !descriptor_heap_enabled {
        return Err(
            "decoded image present requires VK_EXT_descriptor_heap; descriptor-set path is disabled"
                .to_owned(),
        );
    }

    let plane_formats = native_vulkan_vulkanalia_decoded_image_plane_formats(picture_format)?;
    let descriptor_heap_plan = native_vulkan_vulkanalia_descriptor_heap_image_sampler_plan(
        NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanInput {
            image_count: 2,
            properties: descriptor_heap_properties,
        },
    );
    if !descriptor_heap_plan.backend_ready {
        return Err(format!(
            "decoded image present requires a ready VK_EXT_descriptor_heap plan: {:?}",
            descriptor_heap_plan.blocking_reason
        ));
    }

    let sampler_info = native_vulkan_vulkanalia_decoded_image_descriptor_heap_sampler_create_info();
    let descriptor_heap = native_vulkan_vulkanalia_create_decoded_image_present_descriptor_heap(
        device,
        memory_properties,
        resource_image.image,
        plane_formats,
        sampled_array_layer,
        &sampler_info,
        &descriptor_heap_plan,
    )?;
    let descriptor_heap_resources_created = true;
    let descriptor_heap_resource_descriptor_written =
        descriptor_heap.snapshot.resource_descriptor_written;
    let descriptor_heap_sampler_descriptor_written =
        descriptor_heap.snapshot.sampler_descriptor_written;

    Ok(VulkanaliaDecodedImagePresentSamplerResources {
        descriptor_heap,
        snapshot: NativeVulkanVulkanaliaDecodedImagePresentSamplerSnapshot {
            binding: "vulkanalia",
            route: "decoded-image-plane-sampler-present-resource",
            source_image_role: resource_image.snapshot.role,
            picture_format: format!("{picture_format:?}"),
            sampled_array_layer,
            y_plane_format: format!("{:?}", plane_formats.y_view_format),
            uv_plane_format: format!("{:?}", plane_formats.uv_view_format),
            plane_descriptor_count: 2,
            descriptor_heap_only: true,
            descriptor_heap_available: true,
            descriptor_heap_plan_ready: true,
            descriptor_heap_resources_created,
            descriptor_heap_resource_descriptor_written,
            descriptor_heap_sampler_descriptor_written,
            descriptor_heap_plan,
            descriptor_type: "combined-image-sampler-plane-pair",
            image_layout_for_shader: "shader-read-only-optimal",
            present_pass_model: "decoded image planes -> VK_EXT_descriptor_heap Y/UV sampler mapping -> dynamic rendering fullscreen graphics pass -> swapchain",
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
    let plane_formats = native_vulkan_vulkanalia_decoded_image_plane_formats(picture_format)?;
    let sampler_info = native_vulkan_vulkanalia_decoded_image_descriptor_heap_sampler_create_info();
    native_vulkan_vulkanalia_write_decoded_image_present_plane_descriptors(
        device,
        &mut resources.descriptor_heap,
        resource_image.image,
        plane_formats,
        sampled_array_layer,
        &sampler_info,
    )?;
    resources
        .snapshot
        .descriptor_heap_resource_descriptor_written = resources
        .descriptor_heap
        .snapshot
        .resource_descriptor_written;
    resources
        .snapshot
        .descriptor_heap_sampler_descriptor_written = resources
        .descriptor_heap
        .snapshot
        .sampler_descriptor_written;
    resources.snapshot.sampled_array_layer = sampled_array_layer;
    Ok(())
}

pub(super) fn native_vulkan_vulkanalia_destroy_decoded_image_present_sampler_resources(
    device: &Device,
    resources: VulkanaliaDecodedImagePresentSamplerResources,
) {
    native_vulkan_vulkanalia_destroy_descriptor_heap_image_sampler_resources(
        device,
        resources.descriptor_heap,
    );
}

fn native_vulkan_vulkanalia_create_decoded_image_present_descriptor_heap(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    image: vk::Image,
    plane_formats: VulkanaliaDecodedImagePlaneFormats,
    sampled_array_layer: u32,
    sampler_info: &vk::SamplerCreateInfo,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
) -> Result<VulkanaliaDescriptorHeapImageSamplerResources, String> {
    let mut heap_resources =
        native_vulkan_vulkanalia_create_descriptor_heap_image_sampler_resources(
            device,
            memory_properties,
            descriptor_heap_plan,
        )?;
    if let Err(err) = native_vulkan_vulkanalia_write_decoded_image_present_plane_descriptors(
        device,
        &mut heap_resources,
        image,
        plane_formats,
        sampled_array_layer,
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

fn native_vulkan_vulkanalia_write_decoded_image_present_plane_descriptors(
    device: &Device,
    resources: &mut VulkanaliaDescriptorHeapImageSamplerResources,
    image: vk::Image,
    plane_formats: VulkanaliaDecodedImagePlaneFormats,
    sampled_array_layer: u32,
    sampler_info: &vk::SamplerCreateInfo,
) -> Result<(), String> {
    let mut y_view_usage_info = vk::ImageViewUsageCreateInfo::builder()
        .usage(native_vulkan_vulkanalia_decoded_image_plane_sampled_view_usage())
        .build();
    let y_view_info = native_vulkan_vulkanalia_decoded_image_plane_view_create_info(
        image,
        vk::ImageAspectFlags::PLANE_0,
        plane_formats.y_view_format,
        sampled_array_layer,
        &mut y_view_usage_info,
    );
    native_vulkan_vulkanalia_write_descriptor_heap_image_sampler(
        device,
        resources,
        0,
        &y_view_info,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        sampler_info,
    )?;

    let mut uv_view_usage_info = vk::ImageViewUsageCreateInfo::builder()
        .usage(native_vulkan_vulkanalia_decoded_image_plane_sampled_view_usage())
        .build();
    let uv_view_info = native_vulkan_vulkanalia_decoded_image_plane_view_create_info(
        image,
        vk::ImageAspectFlags::PLANE_1,
        plane_formats.uv_view_format,
        sampled_array_layer,
        &mut uv_view_usage_info,
    );
    native_vulkan_vulkanalia_write_descriptor_heap_image_sampler(
        device,
        resources,
        1,
        &uv_view_info,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        sampler_info,
    )
}

fn native_vulkan_vulkanalia_decoded_image_descriptor_heap_sampler_create_info()
-> vk::SamplerCreateInfo {
    vk::SamplerCreateInfo::builder()
        .mag_filter(vk::Filter::LINEAR)
        .min_filter(vk::Filter::LINEAR)
        .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .min_lod(0.0)
        .max_lod(0.0)
        .build()
}

fn native_vulkan_vulkanalia_decoded_image_plane_view_create_info<'a>(
    image: vk::Image,
    aspect_mask: vk::ImageAspectFlags,
    plane_format: vk::Format,
    sampled_array_layer: u32,
    view_usage_info: &'a mut vk::ImageViewUsageCreateInfo,
) -> vk::ImageViewCreateInfo {
    let subresource_range = vk::ImageSubresourceRange::builder()
        .aspect_mask(aspect_mask)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(sampled_array_layer)
        .layer_count(1)
        .build();
    vk::ImageViewCreateInfo::builder()
        .image(image)
        .view_type(vk::ImageViewType::_2D)
        .format(plane_format)
        .components(native_vulkan_vulkanalia_identity_component_mapping())
        .subresource_range(subresource_range)
        .push_next(view_usage_info)
        .build()
}

fn native_vulkan_vulkanalia_decoded_image_plane_sampled_view_usage() -> vk::ImageUsageFlags {
    vk::ImageUsageFlags::SAMPLED
}

fn native_vulkan_vulkanalia_identity_component_mapping() -> vk::ComponentMapping {
    vk::ComponentMapping {
        r: vk::ComponentSwizzle::IDENTITY,
        g: vk::ComponentSwizzle::IDENTITY,
        b: vk::ComponentSwizzle::IDENTITY,
        a: vk::ComponentSwizzle::IDENTITY,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VulkanaliaDecodedImagePlaneFormats {
    y_view_format: vk::Format,
    uv_view_format: vk::Format,
}

fn native_vulkan_vulkanalia_decoded_image_plane_formats(
    picture_format: vk::Format,
) -> Result<VulkanaliaDecodedImagePlaneFormats, String> {
    match picture_format {
        vk::Format::G8_B8R8_2PLANE_420_UNORM => Ok(VulkanaliaDecodedImagePlaneFormats {
            y_view_format: vk::Format::R8_UNORM,
            uv_view_format: vk::Format::R8G8_UNORM,
        }),
        vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16 => {
            Ok(VulkanaliaDecodedImagePlaneFormats {
                y_view_format: vk::Format::R16_UNORM,
                uv_view_format: vk::Format::R16G16_UNORM,
            })
        }
        _ => Err(format!(
            "{picture_format:?} decoded video plane sampling is not implemented"
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoded_image_present_plane_formats_match_nv12_and_p010() {
        let nv12 = native_vulkan_vulkanalia_decoded_image_plane_formats(
            vk::Format::G8_B8R8_2PLANE_420_UNORM,
        )
        .unwrap();
        assert_eq!(nv12.y_view_format, vk::Format::R8_UNORM);
        assert_eq!(nv12.uv_view_format, vk::Format::R8G8_UNORM);

        let p010 = native_vulkan_vulkanalia_decoded_image_plane_formats(
            vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16,
        )
        .unwrap();
        assert_eq!(p010.y_view_format, vk::Format::R16_UNORM);
        assert_eq!(p010.uv_view_format, vk::Format::R16G16_UNORM);

        assert!(
            native_vulkan_vulkanalia_decoded_image_plane_formats(vk::Format::R8G8B8A8_UNORM)
                .is_err()
        );
    }
}
