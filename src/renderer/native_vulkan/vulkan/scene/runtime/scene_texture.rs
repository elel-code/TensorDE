//! GPU image creation for material textures lowered into `.gscene`.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/exe/texture-and-format.md`
//! - `references/godot/servers/rendering/storage/texture_storage.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use std::collections::BTreeSet;

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use crate::engine::scene::{SceneResourceId, SceneStorage, SceneTextureFormat};
use crate::renderer::native_vulkan::vulkan::{
    NativeVulkanVulkanaliaImageMipUpload, NativeVulkanVulkanaliaRecordedImageUpload,
    native_vulkan_vulkanalia_create_sampled_image_with_recorded_staging_upload,
    native_vulkan_vulkanalia_destroy_buffer, native_vulkan_vulkanalia_destroy_image,
};

use super::sampled_binding::{SceneSampledImageBindingPlan, SceneSampledImageSource};

pub(in crate::renderer::native_vulkan) struct SceneTextureImageResource {
    pub resource: SceneResourceId,
    pub format: vk::Format,
    pub mip_levels: u32,
    pub sampler_flags: u32,
    pub upload: NativeVulkanVulkanaliaRecordedImageUpload,
}

pub(in crate::renderer::native_vulkan) fn create_scene_texture_images(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    command_buffer: vk::CommandBuffer,
    storage: &SceneStorage,
    binding_cycle: &[SceneSampledImageBindingPlan],
) -> Result<Vec<SceneTextureImageResource>, String> {
    let resource_ids = binding_cycle
        .iter()
        .flat_map(|plan| plan.sources.iter())
        .filter_map(|source| match source {
            SceneSampledImageSource::SceneTexture { resource } => Some(*resource),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    let mut resources = Vec::with_capacity(resource_ids.len());
    for resource in resource_ids {
        let result = create_scene_texture_image(
            device,
            memory_properties,
            command_buffer,
            storage,
            resource,
        );
        match result {
            Ok(image) => resources.push(image),
            Err(err) => {
                destroy_scene_texture_images(device, resources);
                return Err(err);
            }
        }
    }
    Ok(resources)
}

fn create_scene_texture_image(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    command_buffer: vk::CommandBuffer,
    storage: &SceneStorage,
    resource: SceneResourceId,
) -> Result<SceneTextureImageResource, String> {
    let texture = storage.texture(resource).ok_or_else(|| {
        format!(
            "scene material texture resource {} has no texture record",
            resource.0
        )
    })?;
    let format = scene_texture_vk_format(texture.format);
    let payload = storage.texture_payload(texture);
    let mips = storage
        .texture_mips(texture)
        .iter()
        .map(|mip| {
            let local_offset = mip
                .payload_offset
                .checked_sub(texture.payload_offset)
                .ok_or_else(|| {
                    format!(
                        "scene texture resource {} mip precedes its payload range",
                        resource.0
                    )
                })?;
            Ok(NativeVulkanVulkanaliaImageMipUpload {
                buffer_offset: local_offset,
                byte_count: mip.payload_len,
                width: mip.width,
                height: mip.height,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    let upload = native_vulkan_vulkanalia_create_sampled_image_with_recorded_staging_upload(
        device,
        memory_properties,
        command_buffer,
        "scene-material-texture",
        format,
        texture.storage_width,
        texture.storage_height,
        texture.mip_count,
        payload,
        &mips,
    )?;
    Ok(SceneTextureImageResource {
        resource,
        format,
        mip_levels: texture.mip_count,
        sampler_flags: texture.sampler_flags,
        upload,
    })
}

pub(in crate::renderer::native_vulkan) fn destroy_scene_texture_images(
    device: &Device,
    resources: Vec<SceneTextureImageResource>,
) {
    for resource in resources {
        if let Some(staging) = resource.upload.staging {
            native_vulkan_vulkanalia_destroy_buffer(device, staging);
        }
        native_vulkan_vulkanalia_destroy_image(device, resource.upload.image);
    }
}

pub(in crate::renderer::native_vulkan) fn release_scene_texture_staging(
    device: &Device,
    resources: &mut [SceneTextureImageResource],
) {
    for resource in resources {
        if let Some(staging) = resource.upload.staging.take() {
            native_vulkan_vulkanalia_destroy_buffer(device, staging);
        }
    }
}

pub(in crate::renderer::native_vulkan) fn scene_texture_image<'a>(
    resources: &'a [SceneTextureImageResource],
    resource: SceneResourceId,
) -> Option<&'a SceneTextureImageResource> {
    resources.iter().find(|image| image.resource == resource)
}

pub(in crate::renderer::native_vulkan) fn scene_texture_image_view_info(
    resource: &SceneTextureImageResource,
) -> vk::ImageViewCreateInfo {
    vk::ImageViewCreateInfo::builder()
        .image(resource.upload.image.image)
        .view_type(vk::ImageViewType::_2D)
        .format(resource.format)
        .components(identity_component_mapping())
        .subresource_range(color_subresource_range(resource.mip_levels))
        .build()
}

pub(in crate::renderer::native_vulkan) fn scene_texture_sampler_info(
    resource: &SceneTextureImageResource,
) -> vk::SamplerCreateInfo {
    let address_mode = scene_texture_address_mode(resource.sampler_flags);
    vk::SamplerCreateInfo::builder()
        .mag_filter(vk::Filter::LINEAR)
        .min_filter(vk::Filter::LINEAR)
        .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
        .address_mode_u(address_mode)
        .address_mode_v(address_mode)
        .address_mode_w(address_mode)
        .max_lod(resource.mip_levels.saturating_sub(1) as f32)
        .build()
}

fn scene_texture_address_mode(sampler_flags: u32) -> vk::SamplerAddressMode {
    // WE sampler helper 0x140099980 maps payload/config bit 1 to D3D address
    // mode 3 (clamp); its absence maps to mode 1 (wrap).
    if sampler_flags & 0x2 != 0 {
        vk::SamplerAddressMode::CLAMP_TO_EDGE
    } else {
        vk::SamplerAddressMode::REPEAT
    }
}

pub(in crate::renderer::native_vulkan) fn scene_texture_memory_bytes(
    resources: &[SceneTextureImageResource],
) -> u64 {
    resources
        .iter()
        .map(|resource| resource.upload.image.snapshot.memory_size)
        .sum()
}

fn scene_texture_vk_format(format: SceneTextureFormat) -> vk::Format {
    match format {
        SceneTextureFormat::Rgba8Unorm => vk::Format::R8G8B8A8_UNORM,
        SceneTextureFormat::Rg8Unorm => vk::Format::R8G8_UNORM,
        SceneTextureFormat::R8Unorm => vk::Format::R8_UNORM,
        SceneTextureFormat::Bc1RgbaUnormBlock => vk::Format::BC1_RGBA_UNORM_BLOCK,
        SceneTextureFormat::Bc2UnormBlock => vk::Format::BC2_UNORM_BLOCK,
        SceneTextureFormat::Bc3UnormBlock => vk::Format::BC3_UNORM_BLOCK,
        SceneTextureFormat::Bc4UnormBlock => vk::Format::BC4_UNORM_BLOCK,
        SceneTextureFormat::Bc5UnormBlock => vk::Format::BC5_UNORM_BLOCK,
        SceneTextureFormat::Bc7UnormBlock => vk::Format::BC7_UNORM_BLOCK,
    }
}

fn identity_component_mapping() -> vk::ComponentMapping {
    vk::ComponentMapping::builder()
        .r(vk::ComponentSwizzle::IDENTITY)
        .g(vk::ComponentSwizzle::IDENTITY)
        .b(vk::ComponentSwizzle::IDENTITY)
        .a(vk::ComponentSwizzle::IDENTITY)
        .build()
}

fn color_subresource_range(mip_levels: u32) -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(mip_levels)
        .base_array_layer(0)
        .layer_count(1)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_gpu_formats_map_to_vulkan_block_formats() {
        assert_eq!(
            scene_texture_vk_format(SceneTextureFormat::Bc7UnormBlock),
            vk::Format::BC7_UNORM_BLOCK
        );
        assert_eq!(
            scene_texture_vk_format(SceneTextureFormat::Bc4UnormBlock),
            vk::Format::BC4_UNORM_BLOCK
        );
        assert_eq!(
            scene_texture_vk_format(SceneTextureFormat::Bc5UnormBlock),
            vk::Format::BC5_UNORM_BLOCK
        );
    }

    #[test]
    fn we_sampler_seed_bit_one_selects_clamp_instead_of_wrap() {
        assert_eq!(
            scene_texture_address_mode(0),
            vk::SamplerAddressMode::REPEAT
        );
        assert_eq!(
            scene_texture_address_mode(0x8),
            vk::SamplerAddressMode::REPEAT
        );
        assert_eq!(
            scene_texture_address_mode(0x2),
            vk::SamplerAddressMode::CLAMP_TO_EDGE
        );
        assert_eq!(
            scene_texture_address_mode(0xa),
            vk::SamplerAddressMode::CLAMP_TO_EDGE
        );
    }
}
