//! Vulkan descriptor heap write helpers for scene textures.
//!
//! References:
//! - `reverse-engineered/docs/tex-format.md`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`
//! - `src/renderer/native_vulkan/vulkan/core/descriptor_heap.rs`

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use crate::renderer::native_vulkan::vulkan::{
    VulkanaliaDescriptorHeapImageSamplerResources,
    native_vulkan_vulkanalia_write_descriptor_heap_image_sampler,
};

use super::frame_plan::{
    NativeVulkanSceneTextureHeapFramePlan, NativeVulkanSceneTextureHeapImageBinding,
};

pub(super) fn write_scene_texture_heap_descriptors(
    device: &Device,
    resources: &mut VulkanaliaDescriptorHeapImageSamplerResources,
    frame_plan: &NativeVulkanSceneTextureHeapFramePlan,
) -> Result<(), String> {
    for binding in &frame_plan.bindings {
        let view_info = scene_texture_heap_image_view_create_info(binding);
        let sampler_info = scene_texture_heap_sampler_create_info(binding.mip_count);
        native_vulkan_vulkanalia_write_descriptor_heap_image_sampler(
            device,
            resources,
            binding.heap_index,
            &view_info,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            &sampler_info,
        )?;
    }
    Ok(())
}

fn scene_texture_heap_image_view_create_info(
    binding: &NativeVulkanSceneTextureHeapImageBinding,
) -> vk::ImageViewCreateInfo {
    let subresource_range = vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(binding.mip_count)
        .base_array_layer(0)
        .layer_count(1)
        .build();
    vk::ImageViewCreateInfo::builder()
        .image(binding.image)
        .view_type(vk::ImageViewType::_2D)
        .format(binding.format)
        .components(identity_component_mapping())
        .subresource_range(subresource_range)
        .build()
}

fn scene_texture_heap_sampler_create_info(mip_count: u32) -> vk::SamplerCreateInfo {
    vk::SamplerCreateInfo::builder()
        .mag_filter(vk::Filter::LINEAR)
        .min_filter(vk::Filter::LINEAR)
        .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .min_lod(0.0)
        .max_lod(mip_count.saturating_sub(1) as f32)
        .build()
}

fn identity_component_mapping() -> vk::ComponentMapping {
    vk::ComponentMapping {
        r: vk::ComponentSwizzle::IDENTITY,
        g: vk::ComponentSwizzle::IDENTITY,
        b: vk::ComponentSwizzle::IDENTITY,
        a: vk::ComponentSwizzle::IDENTITY,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::SceneResourceId;
    use vulkanalia::vk::Handle;

    #[test]
    fn texture_heap_view_create_info_uses_full_mip_range() {
        let binding = NativeVulkanSceneTextureHeapImageBinding {
            resource: SceneResourceId(7),
            heap_index: 0,
            image: vk::Image::from_raw(107),
            format: vk::Format::R8G8B8A8_UNORM,
            width: 512,
            height: 256,
            mip_count: 4,
        };

        let view_info = scene_texture_heap_image_view_create_info(&binding);
        let sampler_info = scene_texture_heap_sampler_create_info(binding.mip_count);

        assert_eq!(view_info.image, vk::Image::from_raw(107));
        assert_eq!(view_info.view_type, vk::ImageViewType::_2D);
        assert_eq!(view_info.format, vk::Format::R8G8B8A8_UNORM);
        assert_eq!(view_info.subresource_range.level_count, 4);
        assert_eq!(sampler_info.mipmap_mode, vk::SamplerMipmapMode::LINEAR);
        assert_eq!(sampler_info.max_lod, 3.0);
    }
}
