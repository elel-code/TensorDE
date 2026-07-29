//! Scene GPU image, sampler, upload-staging, and resource teardown lifecycle.

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use crate::renderer::native_vulkan::{
    NativeVulkanVulkanaliaImage, NativeVulkanVulkanaliaImageMipUpload,
    NativeVulkanVulkanaliaRecordedImageUpload,
    native_vulkan_vulkanalia_create_sampled_image_with_recorded_staging_upload,
    native_vulkan_vulkanalia_destroy_buffer,
    native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources,
    native_vulkan_vulkanalia_destroy_image,
};

use super::pipeline::destroy_scene_pipelines;
use super::{
    SCENE_WHITE_TEXTURE_BYTES, SceneGpuFrameResources, SceneGpuResources, effect_target,
    scene_texture,
};

pub(super) fn create_white_texture_upload(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    command_buffer: vk::CommandBuffer,
) -> Result<NativeVulkanVulkanaliaRecordedImageUpload, String> {
    let mip = NativeVulkanVulkanaliaImageMipUpload {
        buffer_offset: 0,
        byte_count: SCENE_WHITE_TEXTURE_BYTES.len() as u64,
        width: 1,
        height: 1,
    };
    native_vulkan_vulkanalia_create_sampled_image_with_recorded_staging_upload(
        device,
        memory_properties,
        command_buffer,
        "scene-white-fallback-texture",
        vk::Format::R8G8B8A8_UNORM,
        1,
        1,
        1,
        SCENE_WHITE_TEXTURE_BYTES,
        &[mip],
    )
}

pub(super) fn scene_white_image_view_info(
    image: &NativeVulkanVulkanaliaImage,
) -> vk::ImageViewCreateInfo {
    vk::ImageViewCreateInfo::builder()
        .image(image.image)
        .view_type(vk::ImageViewType::_2D)
        .format(vk::Format::R8G8B8A8_UNORM)
        .components(identity_component_mapping())
        .subresource_range(color_subresource_range())
        .build()
}

pub(super) fn scene_color_image_view_info(
    image: vk::Image,
    format: vk::Format,
) -> vk::ImageViewCreateInfo {
    vk::ImageViewCreateInfo::builder()
        .image(image)
        .view_type(vk::ImageViewType::_2D)
        .format(format)
        .components(identity_component_mapping())
        .subresource_range(color_subresource_range())
        .build()
}

pub(super) fn scene_sampled_sampler_info() -> vk::SamplerCreateInfo {
    vk::SamplerCreateInfo::builder()
        .mag_filter(vk::Filter::LINEAR)
        .min_filter(vk::Filter::LINEAR)
        .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .max_lod(0.0)
        .build()
}

pub(super) fn destroy_scene_gpu_resources(device: &Device, resources: SceneGpuResources) {
    if let Some(particle_resources) = resources.particle_resources {
        super::particle_resources::destroy_scene_particle_gpu_resources(device, particle_resources);
    }
    destroy_scene_pipelines(device, resources.pipelines);
    super::scene_color_msaa::destroy_scene_color_msaa_targets(
        device,
        resources.scene_color_msaa_targets,
    );
    destroy_scene_gpu_frame_resources(device, resources.frame_resources);
    scene_texture::destroy_scene_texture_images(device, resources.scene_textures);
    effect_target::destroy_scene_effect_target_images(device, resources.effect_targets);
    if let Some(upload) = resources.white_upload {
        destroy_recorded_image_upload(device, upload);
    }
    resources.mesh_uploads.destroy(device);
}

pub(super) fn destroy_scene_gpu_frame_resources(
    device: &Device,
    frame_resources: Vec<SceneGpuFrameResources>,
) {
    for resources in frame_resources {
        native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources(
            device,
            resources.descriptor_heap,
        );
        if let Some(buffer) = resources.material_buffer {
            native_vulkan_vulkanalia_destroy_buffer(device, buffer);
        }
        if let Some(buffer) = resources.skinning_buffer {
            native_vulkan_vulkanalia_destroy_buffer(device, buffer);
        }
        if let Some(buffer) = resources.scene_owned_uniform_buffer {
            native_vulkan_vulkanalia_destroy_buffer(device, buffer);
        }
        native_vulkan_vulkanalia_destroy_buffer(device, resources.transform_buffer);
    }
}

pub(super) fn destroy_recorded_image_upload(
    device: &Device,
    upload: NativeVulkanVulkanaliaRecordedImageUpload,
) {
    if let Some(staging) = upload.staging {
        native_vulkan_vulkanalia_destroy_buffer(device, staging);
    }
    native_vulkan_vulkanalia_destroy_image(device, upload.image);
}

pub(super) fn release_scene_upload_staging(device: &Device, resources: &mut SceneGpuResources) {
    resources.mesh_uploads.release_staging(device);
    if let Some(particle_resources) = resources.particle_resources.as_mut() {
        super::particle_resources::release_scene_particle_staging(device, particle_resources);
    }
    if let Some(upload) = &mut resources.white_upload
        && let Some(staging) = upload.staging.take()
    {
        native_vulkan_vulkanalia_destroy_buffer(device, staging);
    }
    scene_texture::release_scene_texture_staging(device, &mut resources.scene_textures);
}

pub(super) fn color_subresource_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1)
        .build()
}

pub(super) fn identity_component_mapping() -> vk::ComponentMapping {
    vk::ComponentMapping {
        r: vk::ComponentSwizzle::IDENTITY,
        g: vk::ComponentSwizzle::IDENTITY,
        b: vk::ComponentSwizzle::IDENTITY,
        a: vk::ComponentSwizzle::IDENTITY,
    }
}
