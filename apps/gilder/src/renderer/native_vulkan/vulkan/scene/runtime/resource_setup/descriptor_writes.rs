//! Retained scene descriptor writes split by image access semantics.

use super::super::*;
use super::super::input_attachment_binding::SceneInputAttachmentSource;
use crate::renderer::native_vulkan::native_vulkan_vulkanalia_write_descriptor_heap_resource_input_attachment;

pub(super) fn write_scene_color_snapshot_descriptors(
    device: &Device,
    descriptor_heap: &mut VulkanaliaDescriptorHeapResourceResources,
    draw_commands: &[SceneGpuDrawCommand],
    sampled_binding_plan: &SceneSampledImageBindingPlan,
    scene_color_image: vk::Image,
    scene_color_format: vk::Format,
) -> Result<usize, String> {
    let image_view_info = scene_color_image_view_info(scene_color_image, scene_color_format);
    let sampler_info = scene_sampled_sampler_info();
    let mut update_count = 0usize;
    for (draw_index, draw) in draw_commands.iter().enumerate() {
        for sampled_index in 0..sampled_binding_plan.sampled_slot_count {
            if sampled_binding_plan.source(draw_index, sampled_index)
                != Some(SceneSampledImageSource::SceneColorSnapshot)
            {
                continue;
            }
            native_vulkan_vulkanalia_write_descriptor_heap_resource_image_sampler(
                device,
                descriptor_heap,
                draw.sampled_resource_descriptor_base + sampled_index,
                draw.sampler_descriptor_base + sampled_index,
                &image_view_info,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                &sampler_info,
            )?;
            update_count += 1;
        }
    }
    Ok(update_count)
}

pub(super) fn write_scene_sampled_descriptors(
    device: &Device,
    descriptor_heap: &mut VulkanaliaDescriptorHeapResourceResources,
    draw_commands: &[SceneGpuDrawCommand],
    white_image: Option<&NativeVulkanVulkanaliaImage>,
    scene_textures: &[scene_texture::SceneTextureImageResource],
    effect_targets: &[effect_target::SceneEffectTargetImageResource],
    sampled_binding_plan: &SceneSampledImageBindingPlan,
    scene_color: Option<(vk::Image, vk::Format)>,
) -> Result<(), String> {
    let fallback_image_view_info = white_image.map(scene_white_image_view_info);
    let fallback_sampler_info = scene_sampled_sampler_info();
    for (draw_index, draw) in draw_commands.iter().enumerate() {
        for sampled_index in 0..sampled_binding_plan.sampled_slot_count {
            let source = sampled_binding_plan
                .source(draw_index, sampled_index)
                .ok_or_else(|| {
                    format!(
                        "scene draw {draw_index} sampled descriptor {sampled_index} has no binding plan"
                    )
                })?;
            let (image_view_info, sampler_info) = match source {
                SceneSampledImageSource::FallbackWhite => (
                    fallback_image_view_info.ok_or_else(|| {
                        "scene fallback sampled binding has no fallback texture".to_owned()
                    })?,
                    fallback_sampler_info,
                ),
                SceneSampledImageSource::SceneTexture { resource } => {
                    let texture = scene_texture::scene_texture_image(scene_textures, resource)
                        .ok_or_else(|| {
                            format!(
                                "scene sampled texture resource {} has no GPU image",
                                resource.0
                            )
                        })?;
                    (
                        scene_texture::scene_texture_image_view_info(texture),
                        scene_texture::scene_texture_sampler_info(texture),
                    )
                }
                SceneSampledImageSource::SceneColorSnapshot => {
                    let (image, format) = scene_color.ok_or_else(|| {
                        "scene color snapshot descriptor is unavailable before image acquire"
                            .to_owned()
                    })?;
                    (
                        scene_color_image_view_info(image, format),
                        fallback_sampler_info,
                    )
                }
                SceneSampledImageSource::EffectTarget {
                    physical_slot,
                    batch_atlas_tile,
                } => {
                    let resource = effect_targets
                        .iter()
                        .find(|resource| resource.plan.physical_slot == physical_slot)
                        .ok_or_else(|| {
                            format!(
                                "scene sampled effect target physical slot {physical_slot} has no image"
                            )
                        })?;
                    (
                        effect_target::effect_target_image_view_info(resource, batch_atlas_tile),
                        fallback_sampler_info,
                    )
                }
                SceneSampledImageSource::VideoFrame { media_instance } => {
                    return Err(format!(
                        "scene video media instance {media_instance} has no external frame resource for descriptor resolution"
                    ));
                }
            };
            native_vulkan_vulkanalia_write_descriptor_heap_resource_image_sampler(
                device,
                descriptor_heap,
                draw.sampled_resource_descriptor_base + sampled_index,
                draw.sampler_descriptor_base + sampled_index,
                &image_view_info,
                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                &sampler_info,
            )?;
        }
    }
    Ok(())
}

pub(super) fn write_scene_input_attachment_descriptors(
    device: &Device,
    descriptor_heap: &mut VulkanaliaDescriptorHeapResourceResources,
    draw_commands: &[SceneGpuDrawCommand],
    effect_targets: &[effect_target::SceneEffectTargetImageResource],
    input_attachment_binding_plan: &SceneInputAttachmentBindingPlan,
) -> Result<(), String> {
    for (draw_index, draw) in draw_commands.iter().enumerate() {
        for input_index in 0..input_attachment_binding_plan.input_attachment_slot_count {
            let Some(SceneInputAttachmentSource::EffectTarget { physical_slot, batch_atlas_tile }) =
                input_attachment_binding_plan.source(draw_index, input_index)
            else {
                continue;
            };
            if batch_atlas_tile != 0 {
                return Err(format!(
                    "scene draw {draw_index} input attachment {input_index} has unsupported atlas tile {batch_atlas_tile}"
                ));
            }
            let resource = effect_targets
                .iter()
                .find(|resource| resource.plan.physical_slot == physical_slot)
                .ok_or_else(|| {
                    format!(
                        "scene input-attachment physical slot {physical_slot} has no image"
                    )
                })?;
            let image_view_info = effect_target::effect_target_image_view_info(resource, 0);
            native_vulkan_vulkanalia_write_descriptor_heap_resource_input_attachment(
                device,
                descriptor_heap,
                draw.input_attachment_resource_descriptor_base + input_index,
                &image_view_info,
                vk::ImageLayout::RENDERING_LOCAL_READ,
            )?;
        }
    }
    Ok(())
}
