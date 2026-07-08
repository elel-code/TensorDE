//! Vulkan descriptor heap write helpers for WE auxiliary material heap slices.
//!
//! References:
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`
//! - `src/renderer/native_vulkan/vulkan/core/descriptor_heap.rs`

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use crate::renderer::native_vulkan::vulkan::{
    VulkanaliaDescriptorHeapResourceResources,
    native_vulkan_vulkanalia_write_descriptor_heap_resource_image_sampler,
};

use super::{
    NativeVulkanSceneLayerAuxMaterialResourceHeapDescriptorBinding,
    NativeVulkanSceneLayerAuxMaterialResourceHeapFramePlan,
};

pub(super) fn write_scene_layer_aux_material_resource_heap_descriptors(
    device: &Device,
    resources: &mut VulkanaliaDescriptorHeapResourceResources,
    frame_plan: &NativeVulkanSceneLayerAuxMaterialResourceHeapFramePlan,
) -> Result<(), String> {
    for binding in &frame_plan.bindings {
        let NativeVulkanSceneLayerAuxMaterialResourceHeapDescriptorBinding::SampledImage {
            descriptor_index,
            sampler_descriptor_index,
            image,
            format,
            mip_count,
            ..
        } = *binding;
        let view_info =
            layer_aux_material_resource_heap_image_view_create_info(image, format, mip_count);
        let sampler_info = layer_aux_material_resource_heap_sampler_create_info(mip_count);
        native_vulkan_vulkanalia_write_descriptor_heap_resource_image_sampler(
            device,
            resources,
            descriptor_index,
            sampler_descriptor_index,
            &view_info,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            &sampler_info,
        )?;
    }
    Ok(())
}

fn layer_aux_material_resource_heap_image_view_create_info(
    image: vk::Image,
    format: vk::Format,
    mip_count: u32,
) -> vk::ImageViewCreateInfo {
    let subresource_range = vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(mip_count)
        .base_array_layer(0)
        .layer_count(1)
        .build();
    vk::ImageViewCreateInfo::builder()
        .image(image)
        .view_type(vk::ImageViewType::_2D)
        .format(format)
        .components(identity_component_mapping())
        .subresource_range(subresource_range)
        .build()
}

fn layer_aux_material_resource_heap_sampler_create_info(mip_count: u32) -> vk::SamplerCreateInfo {
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
    use vulkanalia::vk::Handle;

    #[test]
    fn aux_material_resource_heap_view_create_info_uses_color_image() {
        let view_info = layer_aux_material_resource_heap_image_view_create_info(
            vk::Image::from_raw(88),
            vk::Format::R8G8B8A8_UNORM,
            1,
        );
        let sampler_info = layer_aux_material_resource_heap_sampler_create_info(1);

        assert_eq!(view_info.image, vk::Image::from_raw(88));
        assert_eq!(view_info.view_type, vk::ImageViewType::_2D);
        assert_eq!(view_info.format, vk::Format::R8G8B8A8_UNORM);
        assert_eq!(
            view_info.subresource_range.aspect_mask,
            vk::ImageAspectFlags::COLOR
        );
        assert_eq!(
            sampler_info.address_mode_u,
            vk::SamplerAddressMode::CLAMP_TO_EDGE
        );
        assert_eq!(sampler_info.max_lod, 0.0);
    }
}
