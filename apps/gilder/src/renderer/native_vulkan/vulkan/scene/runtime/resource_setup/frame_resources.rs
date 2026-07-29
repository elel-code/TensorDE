//! Per-frame retained buffers, descriptor heaps, and buffer descriptor writes.

use super::super::*;
use super::descriptor_writes;

pub(super) fn create_additional_scene_frame_resources(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    transform_payload: &[u8],
    material_payload: Option<&[u8]>,
    skinning_payload: Option<&[u8]>,
    scene_owned_uniform_payload: Option<&[u8]>,
    scene_owned_uniform_plan: &SceneOwnedUniformArenaPlan,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    draw_commands: &[SceneGpuDrawCommand],
    white_image: Option<&NativeVulkanVulkanaliaImage>,
    scene_textures: &[scene_texture::SceneTextureImageResource],
    effect_targets: &[effect_target::SceneEffectTargetImageResource],
    sampled_binding_plan: &SceneSampledImageBindingPlan,
    input_attachment_binding_plan: &SceneInputAttachmentBindingPlan,
    initial_scene_color_image: vk::Image,
    target_format: vk::Format,
    particle_resources: Option<&particle_resources::SceneParticleGpuResources>,
    particle_global_descriptor_base: Option<usize>,
) -> Result<SceneGpuFrameResources, String> {
    let transform_buffer = native_vulkan_vulkanalia_create_buffer(
        device,
        memory_properties,
        "scene-draw-transform-uniform-buffer",
        transform_payload.len() as u64,
        vk::BufferUsageFlags::UNIFORM_BUFFER
            | vk::BufferUsageFlags::VERTEX_BUFFER
            | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
        NativeVulkanVulkanaliaBufferMemoryPreference::HostUpload,
        Some(transform_payload),
    )?;
    let material_buffer = match material_payload {
        Some(payload) => match native_vulkan_vulkanalia_create_buffer(
            device,
            memory_properties,
            "scene-material-uniform-buffer",
            payload.len() as u64,
            vk::BufferUsageFlags::UNIFORM_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            NativeVulkanVulkanaliaBufferMemoryPreference::HostUpload,
            Some(payload),
        ) {
            Ok(buffer) => Some(buffer),
            Err(err) => {
                native_vulkan_vulkanalia_destroy_buffer(device, transform_buffer);
                return Err(err);
            }
        },
        None => None,
    };
    let skinning_buffer = match skinning_payload {
        Some(payload) => match native_vulkan_vulkanalia_create_buffer(
            device,
            memory_properties,
            "scene-puppet-bone-storage-buffer",
            payload.len() as u64,
            vk::BufferUsageFlags::STORAGE_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            NativeVulkanVulkanaliaBufferMemoryPreference::HostUpload,
            Some(payload),
        ) {
            Ok(buffer) => Some(buffer),
            Err(err) => {
                if let Some(buffer) = material_buffer {
                    native_vulkan_vulkanalia_destroy_buffer(device, buffer);
                }
                native_vulkan_vulkanalia_destroy_buffer(device, transform_buffer);
                return Err(err);
            }
        },
        None => None,
    };
    let scene_owned_uniform_buffer = match scene_owned_uniform_payload {
        Some(payload) => match native_vulkan_vulkanalia_create_buffer(
            device,
            memory_properties,
            "scene-owned-uniform-arena",
            payload.len() as u64,
            vk::BufferUsageFlags::UNIFORM_BUFFER | vk::BufferUsageFlags::SHADER_DEVICE_ADDRESS,
            NativeVulkanVulkanaliaBufferMemoryPreference::HostUpload,
            Some(payload),
        ) {
            Ok(buffer) => Some(buffer),
            Err(err) => {
                if let Some(buffer) = skinning_buffer {
                    native_vulkan_vulkanalia_destroy_buffer(device, buffer);
                }
                if let Some(buffer) = material_buffer {
                    native_vulkan_vulkanalia_destroy_buffer(device, buffer);
                }
                native_vulkan_vulkanalia_destroy_buffer(device, transform_buffer);
                return Err(err);
            }
        },
        None => None,
    };
    let descriptor_heap = native_vulkan_vulkanalia_create_descriptor_heap_resource_resources(
        device,
        memory_properties,
        descriptor_heap_plan,
    );
    let mut descriptor_heap = match descriptor_heap {
        Ok(resources) => resources,
        Err(err) => {
            if let Some(buffer) = scene_owned_uniform_buffer {
                native_vulkan_vulkanalia_destroy_buffer(device, buffer);
            }
            if let Some(buffer) = material_buffer {
                native_vulkan_vulkanalia_destroy_buffer(device, buffer);
            }
            if let Some(buffer) = skinning_buffer {
                native_vulkan_vulkanalia_destroy_buffer(device, buffer);
            }
            native_vulkan_vulkanalia_destroy_buffer(device, transform_buffer);
            return Err(err);
        }
    };
    if let Err(err) = write_scene_descriptors(
        device,
        &mut descriptor_heap,
        draw_commands,
        &transform_buffer,
        material_buffer.as_ref(),
        skinning_buffer.as_ref(),
        scene_owned_uniform_buffer.as_ref(),
        scene_owned_uniform_plan,
        white_image,
        scene_textures,
        effect_targets,
        sampled_binding_plan,
        input_attachment_binding_plan,
        Some((initial_scene_color_image, target_format)),
    ) {
        native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources(
            device,
            descriptor_heap,
        );
        if let Some(buffer) = scene_owned_uniform_buffer {
            native_vulkan_vulkanalia_destroy_buffer(device, buffer);
        }
        if let Some(buffer) = material_buffer {
            native_vulkan_vulkanalia_destroy_buffer(device, buffer);
        }
        if let Some(buffer) = skinning_buffer {
            native_vulkan_vulkanalia_destroy_buffer(device, buffer);
        }
        native_vulkan_vulkanalia_destroy_buffer(device, transform_buffer);
        return Err(err);
    }
    if let (Some(resources), Some(descriptor_base)) =
        (particle_resources, particle_global_descriptor_base)
        && let Err(err) = particle_resources::write_scene_particle_descriptors(
            device,
            &mut descriptor_heap,
            descriptor_base,
            resources,
        )
    {
        native_vulkan_vulkanalia_destroy_descriptor_heap_resource_resources(
            device,
            descriptor_heap,
        );
        if let Some(buffer) = scene_owned_uniform_buffer {
            native_vulkan_vulkanalia_destroy_buffer(device, buffer);
        }
        if let Some(buffer) = material_buffer {
            native_vulkan_vulkanalia_destroy_buffer(device, buffer);
        }
        if let Some(buffer) = skinning_buffer {
            native_vulkan_vulkanalia_destroy_buffer(device, buffer);
        }
        native_vulkan_vulkanalia_destroy_buffer(device, transform_buffer);
        return Err(err);
    }
    Ok(SceneGpuFrameResources {
        transform_buffer,
        material_buffer,
        skinning_buffer,
        scene_owned_uniform_buffer,
        descriptor_heap,
        image_binding_phase: 0,
    })
}

pub(super) fn write_scene_descriptors(
    device: &Device,
    descriptor_heap: &mut VulkanaliaDescriptorHeapResourceResources,
    draw_commands: &[SceneGpuDrawCommand],
    transform_buffer: &NativeVulkanVulkanaliaBuffer,
    material_buffer: Option<&NativeVulkanVulkanaliaBuffer>,
    skinning_buffer: Option<&NativeVulkanVulkanaliaBuffer>,
    scene_owned_uniform_buffer: Option<&NativeVulkanVulkanaliaBuffer>,
    scene_owned_uniform_plan: &SceneOwnedUniformArenaPlan,
    white_image: Option<&NativeVulkanVulkanaliaImage>,
    scene_textures: &[scene_texture::SceneTextureImageResource],
    effect_targets: &[effect_target::SceneEffectTargetImageResource],
    sampled_binding_plan: &SceneSampledImageBindingPlan,
    input_attachment_binding_plan: &SceneInputAttachmentBindingPlan,
    scene_color: Option<(vk::Image, vk::Format)>,
) -> Result<(), String> {
    for (draw_index, draw) in draw_commands.iter().enumerate() {
        native_vulkan_vulkanalia_write_descriptor_heap_resource_uniform_buffer(
            device,
            descriptor_heap,
            draw.resource_descriptor_base,
            transform_buffer
                .device_address
                .saturating_add(draw_index as u64 * SCENE_DRAW_UNIFORM_BYTES),
            SCENE_DRAW_UNIFORM_BYTES,
        )?;
        if let Some(resource_descriptor_index) = draw.material_resource_descriptor {
            let material_buffer = material_buffer.ok_or_else(|| {
                format!(
                    "scene draw {draw_index} has a material descriptor without a material buffer"
                )
            })?;
            native_vulkan_vulkanalia_write_descriptor_heap_resource_uniform_buffer(
                device,
                descriptor_heap,
                resource_descriptor_index,
                material_buffer
                    .device_address
                    .saturating_add(draw_index as u64 * SCENE_MATERIAL_UNIFORM_BYTES),
                SCENE_MATERIAL_UNIFORM_BYTES,
            )?;
        }
        if let Some(resource_descriptor_index) = draw.skinning_resource_descriptor {
            let skinning_buffer = skinning_buffer.ok_or_else(|| {
                format!(
                    "scene draw {draw_index} has a skinning descriptor without a skinning buffer"
                )
            })?;
            native_vulkan_vulkanalia_write_descriptor_heap_resource_storage_buffer(
                device,
                descriptor_heap,
                resource_descriptor_index,
                skinning_buffer
                    .device_address
                    .saturating_add(draw.skinning_byte_offset),
                draw.skinning_byte_count,
            )?;
        }
    }
    if !scene_owned_uniform_plan.is_empty() {
        let buffer = scene_owned_uniform_buffer.ok_or_else(|| {
            "scene-owned uniform descriptor plan has no retained buffer".to_owned()
        })?;
        for (draw_index, descriptor_lane, byte_offset, byte_size) in
            scene_owned_uniform_plan.descriptor_slices()
        {
            let draw = draw_commands.get(draw_index).ok_or_else(|| {
                format!("scene-owned uniform descriptor draw {draw_index} is missing")
            })?;
            native_vulkan_vulkanalia_write_descriptor_heap_resource_uniform_buffer(
                device,
                descriptor_heap,
                draw.scene_owned_uniform_descriptor_base + descriptor_lane,
                buffer.device_address.saturating_add(byte_offset),
                byte_size,
            )?;
        }
    }
    descriptor_writes::write_scene_sampled_descriptors(
        device,
        descriptor_heap,
        draw_commands,
        white_image,
        scene_textures,
        effect_targets,
        sampled_binding_plan,
        scene_color,
    )?;
    descriptor_writes::write_scene_input_attachment_descriptors(
        device,
        descriptor_heap,
        draw_commands,
        effect_targets,
        input_attachment_binding_plan,
    )
}
