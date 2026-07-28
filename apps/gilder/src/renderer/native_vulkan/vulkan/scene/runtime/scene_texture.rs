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

use crate::engine::scene::{
    SceneResourceId, SceneStorage, SceneTextureFormat, SceneTextureSamplerAddressMode,
    SceneTextureSamplerFilter,
};
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
    pub sampler_filter: SceneTextureSamplerFilter,
    pub sampler_address_mode: SceneTextureSamplerAddressMode,
    pub upload: NativeVulkanVulkanaliaRecordedImageUpload,
}

pub(in crate::renderer::native_vulkan) fn create_scene_texture_images(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    command_buffer: vk::CommandBuffer,
    storage: &SceneStorage,
    binding_cycle: &[SceneSampledImageBindingPlan],
    device_max_sampler_anisotropy_x1: u32,
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
            device_max_sampler_anisotropy_x1,
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
    device_max_sampler_anisotropy_x1: u32,
) -> Result<SceneTextureImageResource, String> {
    let texture = storage.texture(resource).ok_or_else(|| {
        format!(
            "scene material texture resource {} has no texture record",
            resource.0
        )
    })?;
    validate_scene_texture_sampler_support(
        texture.sampler_filter,
        device_max_sampler_anisotropy_x1,
    )?;
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
        sampler_filter: texture.sampler_filter,
        sampler_address_mode: texture.sampler_address_mode,
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
    scene_texture_sampler_create_info(resource.sampler_filter, resource.sampler_address_mode)
}

fn scene_texture_sampler_create_info(
    filter: SceneTextureSamplerFilter,
    address_mode: SceneTextureSamplerAddressMode,
) -> vk::SamplerCreateInfo {
    let (mag_filter, min_filter, mipmap_mode, anisotropy_enabled, max_anisotropy) = match filter {
        SceneTextureSamplerFilter::Point => (
            vk::Filter::NEAREST,
            vk::Filter::NEAREST,
            vk::SamplerMipmapMode::NEAREST,
            false,
            1.0,
        ),
        SceneTextureSamplerFilter::Linear => (
            vk::Filter::LINEAR,
            vk::Filter::LINEAR,
            vk::SamplerMipmapMode::LINEAR,
            false,
            1.0,
        ),
        SceneTextureSamplerFilter::Anisotropic8 => (
            vk::Filter::LINEAR,
            vk::Filter::LINEAR,
            vk::SamplerMipmapMode::LINEAR,
            true,
            8.0,
        ),
    };
    let address_mode = match address_mode {
        SceneTextureSamplerAddressMode::Repeat => vk::SamplerAddressMode::REPEAT,
        SceneTextureSamplerAddressMode::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
        SceneTextureSamplerAddressMode::ClampToTransparentBlackBorder => {
            vk::SamplerAddressMode::CLAMP_TO_BORDER
        }
    };
    vk::SamplerCreateInfo::builder()
        .mag_filter(mag_filter)
        .min_filter(min_filter)
        .mipmap_mode(mipmap_mode)
        .address_mode_u(address_mode)
        .address_mode_v(address_mode)
        .address_mode_w(address_mode)
        .border_color(vk::BorderColor::FLOAT_TRANSPARENT_BLACK)
        .anisotropy_enable(anisotropy_enabled)
        .max_anisotropy(max_anisotropy)
        .max_lod(vk::LOD_CLAMP_NONE)
        .build()
}

fn validate_scene_texture_sampler_support(
    filter: SceneTextureSamplerFilter,
    device_max_anisotropy_x1: u32,
) -> Result<(), String> {
    if filter == SceneTextureSamplerFilter::Anisotropic8 && device_max_anisotropy_x1 < 8 {
        return Err(format!(
            "Vulkan 2026 scene texture requires maxSamplerAnisotropy >= 8, device reports {device_max_anisotropy_x1}"
        ));
    }
    Ok(())
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
    fn typed_single_mip_file_sampler_keeps_anisotropic_eight() {
        let sampler = scene_texture_sampler_create_info(
            SceneTextureSamplerFilter::Anisotropic8,
            SceneTextureSamplerAddressMode::ClampToEdge,
        );

        assert_eq!(sampler.mag_filter, vk::Filter::LINEAR);
        assert_eq!(sampler.min_filter, vk::Filter::LINEAR);
        assert_eq!(sampler.mipmap_mode, vk::SamplerMipmapMode::LINEAR);
        assert_eq!(sampler.address_mode_u, vk::SamplerAddressMode::CLAMP_TO_EDGE);
        assert_eq!(sampler.anisotropy_enable, vk::TRUE);
        assert_eq!(sampler.max_anisotropy, 8.0);
        assert_eq!(sampler.max_lod, vk::LOD_CLAMP_NONE);
    }

    #[test]
    fn typed_point_linear_and_border_sampler_states_map_exactly() {
        let point = scene_texture_sampler_create_info(
            SceneTextureSamplerFilter::Point,
            SceneTextureSamplerAddressMode::Repeat,
        );
        assert_eq!(point.mag_filter, vk::Filter::NEAREST);
        assert_eq!(point.min_filter, vk::Filter::NEAREST);
        assert_eq!(point.mipmap_mode, vk::SamplerMipmapMode::NEAREST);
        assert_eq!(point.anisotropy_enable, vk::FALSE);
        assert_eq!(point.max_anisotropy, 1.0);

        let linear_border = scene_texture_sampler_create_info(
            SceneTextureSamplerFilter::Linear,
            SceneTextureSamplerAddressMode::ClampToTransparentBlackBorder,
        );
        assert_eq!(linear_border.mag_filter, vk::Filter::LINEAR);
        assert_eq!(linear_border.address_mode_u, vk::SamplerAddressMode::CLAMP_TO_BORDER);
        assert_eq!(
            linear_border.border_color,
            vk::BorderColor::FLOAT_TRANSPARENT_BLACK
        );
        assert_eq!(linear_border.anisotropy_enable, vk::FALSE);
    }

    #[test]
    fn anisotropic_eight_is_required_instead_of_silently_clamped() {
        assert!(validate_scene_texture_sampler_support(
            SceneTextureSamplerFilter::Anisotropic8,
            8
        )
        .is_ok());
        assert!(validate_scene_texture_sampler_support(
            SceneTextureSamplerFilter::Anisotropic8,
            7
        )
        .is_err());
    }
}
