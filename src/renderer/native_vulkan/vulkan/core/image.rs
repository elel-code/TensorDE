//! Generic Vulkan sampled image allocation and staging upload helpers.
//!
//! References:
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`
//! - `references/godot/servers/rendering/storage/texture_storage.h`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use super::buffer::{
    NativeVulkanVulkanaliaBuffer, NativeVulkanVulkanaliaBufferMemoryPreference,
    native_vulkan_vulkanalia_create_buffer, native_vulkan_vulkanalia_destroy_buffer,
};
use super::memory::native_vulkan_vulkanalia_bind_image_memory2;
use super::video_session::{
    NativeVulkanVulkanaliaMemoryTypeCandidate, native_vulkan_vulkanalia_memory_type_candidates,
};

const DEVICE_LOCAL_MEMORY_FLAG_BITS: u32 = vk::MemoryPropertyFlags::DEVICE_LOCAL.bits();
const HOST_VISIBLE_MEMORY_FLAG_BITS: u32 = vk::MemoryPropertyFlags::HOST_VISIBLE.bits();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanVulkanaliaImageMipUpload {
    pub buffer_offset: u64,
    pub byte_count: u64,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaImageSnapshot {
    pub role: &'static str,
    pub image_created: bool,
    pub memory_bound: bool,
    pub view_created: bool,
    pub sampler_created: bool,
    pub format: String,
    pub extent: (u32, u32, u32),
    pub mip_levels: u32,
    pub payload_bytes: u64,
    pub memory_size: u64,
    pub memory_alignment: u64,
    pub memory_type_bits: u32,
    pub selected_memory_type_index: u32,
    pub selected_memory_property_flags: Vec<&'static str>,
    pub usage_flags: Vec<&'static str>,
    pub final_layout: &'static str,
    pub payload_uploaded: bool,
}

pub(in crate::renderer::native_vulkan) struct NativeVulkanVulkanaliaImage {
    pub(in crate::renderer::native_vulkan) image: vk::Image,
    pub(in crate::renderer::native_vulkan) memory: vk::DeviceMemory,
    pub(in crate::renderer::native_vulkan) view: vk::ImageView,
    pub(in crate::renderer::native_vulkan) sampler: vk::Sampler,
    pub(in crate::renderer::native_vulkan) snapshot: NativeVulkanVulkanaliaImageSnapshot,
}

unsafe impl Send for NativeVulkanVulkanaliaImage {}

pub(in crate::renderer::native_vulkan) struct NativeVulkanVulkanaliaRecordedImageUpload {
    pub(in crate::renderer::native_vulkan) image: NativeVulkanVulkanaliaImage,
    pub(in crate::renderer::native_vulkan) staging: Option<NativeVulkanVulkanaliaBuffer>,
    pub(in crate::renderer::native_vulkan) copy_recorded: bool,
}

unsafe impl Send for NativeVulkanVulkanaliaRecordedImageUpload {}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_create_sampled_image_with_recorded_staging_upload(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    command_buffer: vk::CommandBuffer,
    role: &'static str,
    format: vk::Format,
    width: u32,
    height: u32,
    mip_levels: u32,
    payload: &[u8],
    mip_uploads: &[NativeVulkanVulkanaliaImageMipUpload],
) -> Result<NativeVulkanVulkanaliaRecordedImageUpload, String> {
    if width == 0 || height == 0 {
        return Err(format!("{role} sampled image requires non-zero extent"));
    }
    if mip_levels == 0 {
        return Err(format!(
            "{role} sampled image requires at least one mip level"
        ));
    }
    if mip_uploads.len() != mip_levels as usize {
        return Err(format!(
            "{role} sampled image upload has {} mip regions, expected {mip_levels}",
            mip_uploads.len()
        ));
    }
    validate_mip_uploads(role, payload.len() as u64, mip_uploads)?;

    let usage = vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED;
    let extent = vk::Extent3D {
        width,
        height,
        depth: 1,
    };
    let image_info = vk::ImageCreateInfo::builder()
        .image_type(vk::ImageType::_2D)
        .format(format)
        .extent(extent)
        .mip_levels(mip_levels)
        .array_layers(1)
        .samples(vk::SampleCountFlags::_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);
    let image = unsafe { device.create_image(&image_info, None) }
        .map_err(|err| format!("vkCreateImage(vulkanalia {role}): {err:?}"))?;

    let result = (|| -> Result<NativeVulkanVulkanaliaRecordedImageUpload, String> {
        let memory_requirements = unsafe { device.get_image_memory_requirements(image) };
        let memory_type_candidates =
            native_vulkan_vulkanalia_memory_type_candidates(memory_properties);
        let memory_type = image_memory_type(
            &memory_type_candidates,
            memory_requirements.memory_type_bits,
        )
        .ok_or_else(|| {
            format!(
                "{role} sampled image has no matching memory type for bits 0x{:08x}",
                memory_requirements.memory_type_bits
            )
        })?;
        let memory_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(memory_requirements.size)
            .memory_type_index(memory_type.index);
        let memory = unsafe { device.allocate_memory(&memory_info, None) }
            .map_err(|err| format!("vkAllocateMemory(vulkanalia {role}): {err:?}"))?;
        if let Err(err) =
            native_vulkan_vulkanalia_bind_image_memory2(device, image, memory, 0, role)
        {
            unsafe {
                device.free_memory(memory, None);
            }
            return Err(err);
        }

        let staging = match native_vulkan_vulkanalia_create_buffer(
            device,
            memory_properties,
            "scene-texture-recorded-staging-upload",
            payload.len() as u64,
            vk::BufferUsageFlags::TRANSFER_SRC,
            NativeVulkanVulkanaliaBufferMemoryPreference::HostUpload,
            Some(payload),
        ) {
            Ok(staging) => staging,
            Err(err) => {
                unsafe {
                    device.free_memory(memory, None);
                }
                return Err(err);
            }
        };

        if let Err(err) = record_sampled_image_payload_upload(
            device,
            command_buffer,
            role,
            &staging,
            image,
            mip_uploads,
        ) {
            native_vulkan_vulkanalia_destroy_buffer(device, staging);
            unsafe {
                device.free_memory(memory, None);
            }
            return Err(err);
        }

        let view = match create_sampled_image_view(device, role, image, format, mip_levels) {
            Ok(view) => view,
            Err(err) => {
                native_vulkan_vulkanalia_destroy_buffer(device, staging);
                unsafe {
                    device.free_memory(memory, None);
                }
                return Err(err);
            }
        };
        let sampler = match create_sampled_image_sampler(device, role, mip_levels) {
            Ok(sampler) => sampler,
            Err(err) => {
                native_vulkan_vulkanalia_destroy_buffer(device, staging);
                unsafe {
                    device.destroy_image_view(view, None);
                    device.free_memory(memory, None);
                }
                return Err(err);
            }
        };

        Ok(NativeVulkanVulkanaliaRecordedImageUpload {
            image: NativeVulkanVulkanaliaImage {
                image,
                memory,
                view,
                sampler,
                snapshot: NativeVulkanVulkanaliaImageSnapshot {
                    role,
                    image_created: true,
                    memory_bound: true,
                    view_created: true,
                    sampler_created: true,
                    format: format!("{format:?}"),
                    extent: (width, height, 1),
                    mip_levels,
                    payload_bytes: payload.len() as u64,
                    memory_size: memory_requirements.size,
                    memory_alignment: memory_requirements.alignment,
                    memory_type_bits: memory_requirements.memory_type_bits,
                    selected_memory_type_index: memory_type.index,
                    selected_memory_property_flags: memory_property_flag_labels(
                        memory_type.property_flags_bits,
                    ),
                    usage_flags: image_usage_flag_labels(usage),
                    final_layout: "shader-read-only-optimal",
                    payload_uploaded: true,
                },
            },
            staging: Some(staging),
            copy_recorded: true,
        })
    })();

    if result.is_err() {
        unsafe {
            device.destroy_image(image, None);
        }
    }
    result
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_create_color_attachment_sampled_image(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    role: &'static str,
    format: vk::Format,
    width: u32,
    height: u32,
) -> Result<NativeVulkanVulkanaliaImage, String> {
    if format == vk::Format::UNDEFINED {
        return Err(format!("{role} image requires a defined format"));
    }
    if width == 0 || height == 0 {
        return Err(format!("{role} image requires non-zero extent"));
    }

    let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT
        | vk::ImageUsageFlags::SAMPLED
        | vk::ImageUsageFlags::TRANSFER_SRC
        | vk::ImageUsageFlags::TRANSFER_DST;
    let extent = vk::Extent3D {
        width,
        height,
        depth: 1,
    };
    let image_info = vk::ImageCreateInfo::builder()
        .image_type(vk::ImageType::_2D)
        .format(format)
        .extent(extent)
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);
    let image = unsafe { device.create_image(&image_info, None) }
        .map_err(|err| format!("vkCreateImage(vulkanalia {role}): {err:?}"))?;

    let result = (|| -> Result<NativeVulkanVulkanaliaImage, String> {
        let memory_requirements = unsafe { device.get_image_memory_requirements(image) };
        let memory_type_candidates =
            native_vulkan_vulkanalia_memory_type_candidates(memory_properties);
        let memory_type = image_memory_type(
            &memory_type_candidates,
            memory_requirements.memory_type_bits,
        )
        .ok_or_else(|| {
            format!(
                "{role} image has no matching memory type for bits 0x{:08x}",
                memory_requirements.memory_type_bits
            )
        })?;
        let memory_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(memory_requirements.size)
            .memory_type_index(memory_type.index);
        let memory = unsafe { device.allocate_memory(&memory_info, None) }
            .map_err(|err| format!("vkAllocateMemory(vulkanalia {role}): {err:?}"))?;
        if let Err(err) =
            native_vulkan_vulkanalia_bind_image_memory2(device, image, memory, 0, role)
        {
            unsafe {
                device.free_memory(memory, None);
            }
            return Err(err);
        }

        let view = match create_sampled_image_view(device, role, image, format, 1) {
            Ok(view) => view,
            Err(err) => {
                unsafe {
                    device.free_memory(memory, None);
                }
                return Err(err);
            }
        };
        let sampler = match create_sampled_image_sampler(device, role, 1) {
            Ok(sampler) => sampler,
            Err(err) => {
                unsafe {
                    device.destroy_image_view(view, None);
                    device.free_memory(memory, None);
                }
                return Err(err);
            }
        };

        Ok(NativeVulkanVulkanaliaImage {
            image,
            memory,
            view,
            sampler,
            snapshot: NativeVulkanVulkanaliaImageSnapshot {
                role,
                image_created: true,
                memory_bound: true,
                view_created: true,
                sampler_created: true,
                format: format!("{format:?}"),
                extent: (width, height, 1),
                mip_levels: 1,
                payload_bytes: 0,
                memory_size: memory_requirements.size,
                memory_alignment: memory_requirements.alignment,
                memory_type_bits: memory_requirements.memory_type_bits,
                selected_memory_type_index: memory_type.index,
                selected_memory_property_flags: memory_property_flag_labels(
                    memory_type.property_flags_bits,
                ),
                usage_flags: image_usage_flag_labels(usage),
                final_layout: "scene-graph-tracked",
                payload_uploaded: false,
            },
        })
    })();

    if result.is_err() {
        unsafe {
            device.destroy_image(image, None);
        }
    }
    result
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_create_multisampled_color_attachment_image(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    role: &'static str,
    format: vk::Format,
    width: u32,
    height: u32,
    samples: vk::SampleCountFlags,
) -> Result<NativeVulkanVulkanaliaImage, String> {
    if format == vk::Format::UNDEFINED || width == 0 || height == 0 {
        return Err(format!(
            "{role} image requires a defined format and non-zero extent"
        ));
    }
    if samples == vk::SampleCountFlags::_1 {
        return Err(format!("{role} image requires a multisample count"));
    }

    // Scene-color rendering is interrupted by effect-target work. Preserve the
    // multisample contents across those dynamic-rendering scopes; Vulkan's
    // TRANSIENT_ATTACHMENT usage cannot provide that retained LOAD contract.
    let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::TRANSFER_SRC;
    let image_info = vk::ImageCreateInfo::builder()
        .image_type(vk::ImageType::_2D)
        .format(format)
        .extent(vk::Extent3D {
            width,
            height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(samples)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);
    let image = unsafe { device.create_image(&image_info, None) }
        .map_err(|err| format!("vkCreateImage(vulkanalia {role}): {err:?}"))?;

    let result = (|| -> Result<NativeVulkanVulkanaliaImage, String> {
        let requirements = unsafe { device.get_image_memory_requirements(image) };
        let memory_types = native_vulkan_vulkanalia_memory_type_candidates(memory_properties);
        let memory_type = image_memory_type(&memory_types, requirements.memory_type_bits)
            .ok_or_else(|| {
                format!(
                    "{role} image has no matching memory type for bits 0x{:08x}",
                    requirements.memory_type_bits
                )
            })?;
        let memory_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type.index);
        let memory = unsafe { device.allocate_memory(&memory_info, None) }
            .map_err(|err| format!("vkAllocateMemory(vulkanalia {role}): {err:?}"))?;
        if let Err(err) =
            native_vulkan_vulkanalia_bind_image_memory2(device, image, memory, 0, role)
        {
            unsafe { device.free_memory(memory, None) };
            return Err(err);
        }
        let view = match create_sampled_image_view(device, role, image, format, 1) {
            Ok(view) => view,
            Err(err) => {
                unsafe { device.free_memory(memory, None) };
                return Err(err);
            }
        };

        Ok(NativeVulkanVulkanaliaImage {
            image,
            memory,
            view,
            sampler: vk::Sampler::null(),
            snapshot: NativeVulkanVulkanaliaImageSnapshot {
                role,
                image_created: true,
                memory_bound: true,
                view_created: true,
                sampler_created: false,
                format: format!("{format:?}"),
                extent: (width, height, 1),
                mip_levels: 1,
                payload_bytes: 0,
                memory_size: requirements.size,
                memory_alignment: requirements.alignment,
                memory_type_bits: requirements.memory_type_bits,
                selected_memory_type_index: memory_type.index,
                selected_memory_property_flags: memory_property_flag_labels(
                    memory_type.property_flags_bits,
                ),
                usage_flags: image_usage_flag_labels(usage),
                final_layout: "scene-color-attachment",
                payload_uploaded: false,
            },
        })
    })();

    if result.is_err() {
        unsafe { device.destroy_image(image, None) };
    }
    result
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_destroy_image(
    device: &Device,
    image: NativeVulkanVulkanaliaImage,
) {
    unsafe {
        device.destroy_sampler(image.sampler, None);
        device.destroy_image_view(image.view, None);
        device.destroy_image(image.image, None);
        device.free_memory(image.memory, None);
    }
}

fn record_sampled_image_payload_upload(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    role: &'static str,
    staging: &NativeVulkanVulkanaliaBuffer,
    image: vk::Image,
    mip_uploads: &[NativeVulkanVulkanaliaImageMipUpload],
) -> Result<(), String> {
    if mip_uploads.is_empty() {
        return Err(format!(
            "{role} recorded texture upload requires at least one mip"
        ));
    }

    let transfer_barrier = vk::ImageMemoryBarrier2::builder()
        .src_stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE)
        .src_access_mask(vk::AccessFlags2::empty())
        .dst_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
        .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(color_subresource_range(mip_uploads.len() as u32))
        .build();
    let transfer_barriers = [transfer_barrier];
    let transfer_dependency = vk::DependencyInfo::builder()
        .image_memory_barriers(&transfer_barriers)
        .build();

    let copies = mip_uploads
        .iter()
        .enumerate()
        .map(|(level, mip)| {
            vk::BufferImageCopy::builder()
                .buffer_offset(mip.buffer_offset)
                .buffer_row_length(0)
                .buffer_image_height(0)
                .image_subresource(
                    vk::ImageSubresourceLayers::builder()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .mip_level(level as u32)
                        .base_array_layer(0)
                        .layer_count(1)
                        .build(),
                )
                .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
                .image_extent(vk::Extent3D {
                    width: mip.width,
                    height: mip.height,
                    depth: 1,
                })
                .build()
        })
        .collect::<Vec<_>>();

    let shader_barrier = vk::ImageMemoryBarrier2::builder()
        .src_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
        .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::ALL_GRAPHICS)
        .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(color_subresource_range(mip_uploads.len() as u32))
        .build();
    let shader_barriers = [shader_barrier];
    let shader_dependency = vk::DependencyInfo::builder()
        .image_memory_barriers(&shader_barriers)
        .build();

    unsafe {
        device.cmd_pipeline_barrier2(command_buffer, &transfer_dependency);
        device.cmd_copy_buffer_to_image(
            command_buffer,
            staging.buffer,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &copies,
        );
        device.cmd_pipeline_barrier2(command_buffer, &shader_dependency);
    }

    Ok(())
}

fn create_sampled_image_view(
    device: &Device,
    role: &'static str,
    image: vk::Image,
    format: vk::Format,
    mip_levels: u32,
) -> Result<vk::ImageView, String> {
    let create_info = vk::ImageViewCreateInfo::builder()
        .image(image)
        .view_type(vk::ImageViewType::_2D)
        .format(format)
        .components(identity_component_mapping())
        .subresource_range(color_subresource_range(mip_levels));
    unsafe { device.create_image_view(&create_info, None) }
        .map_err(|err| format!("vkCreateImageView(vulkanalia {role}): {err:?}"))
}

fn create_sampled_image_sampler(
    device: &Device,
    role: &'static str,
    mip_levels: u32,
) -> Result<vk::Sampler, String> {
    let max_lod = mip_levels.saturating_sub(1) as f32;
    let create_info = vk::SamplerCreateInfo::builder()
        .mag_filter(vk::Filter::LINEAR)
        .min_filter(vk::Filter::LINEAR)
        .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
        .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
        .mip_lod_bias(0.0)
        .anisotropy_enable(false)
        .max_anisotropy(1.0)
        .compare_enable(false)
        .min_lod(0.0)
        .max_lod(max_lod);
    unsafe { device.create_sampler(&create_info, None) }
        .map_err(|err| format!("vkCreateSampler(vulkanalia {role}): {err:?}"))
}

fn validate_mip_uploads(
    role: &'static str,
    payload_bytes: u64,
    mip_uploads: &[NativeVulkanVulkanaliaImageMipUpload],
) -> Result<(), String> {
    let mut expected_offset = 0u64;
    for (index, mip) in mip_uploads.iter().enumerate() {
        if mip.width == 0 || mip.height == 0 {
            return Err(format!("{role} mip {index} has zero extent"));
        }
        if mip.buffer_offset != expected_offset {
            return Err(format!(
                "{role} mip {index} starts at {}, expected {expected_offset}",
                mip.buffer_offset
            ));
        }
        expected_offset = expected_offset
            .checked_add(mip.byte_count)
            .ok_or_else(|| format!("{role} mip upload byte offset overflow"))?;
    }
    if expected_offset != payload_bytes {
        return Err(format!(
            "{role} mip uploads cover {expected_offset} bytes, expected {payload_bytes}"
        ));
    }
    Ok(())
}

fn image_memory_type(
    memory_types: &[NativeVulkanVulkanaliaMemoryTypeCandidate],
    allowed_memory_type_bits: u32,
) -> Option<NativeVulkanVulkanaliaMemoryTypeCandidate> {
    image_memory_type_matching(
        memory_types,
        allowed_memory_type_bits,
        DEVICE_LOCAL_MEMORY_FLAG_BITS,
    )
    .or_else(|| {
        image_memory_type_matching(
            memory_types,
            allowed_memory_type_bits,
            HOST_VISIBLE_MEMORY_FLAG_BITS,
        )
    })
}

fn image_memory_type_matching(
    memory_types: &[NativeVulkanVulkanaliaMemoryTypeCandidate],
    allowed_memory_type_bits: u32,
    required_property_flags: u32,
) -> Option<NativeVulkanVulkanaliaMemoryTypeCandidate> {
    memory_types.iter().copied().find(|candidate| {
        let allowed = candidate.index < u32::BITS
            && allowed_memory_type_bits & (1u32 << candidate.index) != 0;
        let properties_match =
            candidate.property_flags_bits & required_property_flags == required_property_flags;
        allowed && properties_match
    })
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

fn identity_component_mapping() -> vk::ComponentMapping {
    vk::ComponentMapping {
        r: vk::ComponentSwizzle::IDENTITY,
        g: vk::ComponentSwizzle::IDENTITY,
        b: vk::ComponentSwizzle::IDENTITY,
        a: vk::ComponentSwizzle::IDENTITY,
    }
}

fn image_usage_flag_labels(flags: vk::ImageUsageFlags) -> Vec<&'static str> {
    [
        (vk::ImageUsageFlags::TRANSFER_SRC.bits(), "transfer-src"),
        (vk::ImageUsageFlags::TRANSFER_DST.bits(), "transfer-dst"),
        (vk::ImageUsageFlags::SAMPLED.bits(), "sampled"),
        (vk::ImageUsageFlags::STORAGE.bits(), "storage"),
        (
            vk::ImageUsageFlags::COLOR_ATTACHMENT.bits(),
            "color-attachment",
        ),
        (
            vk::ImageUsageFlags::TRANSIENT_ATTACHMENT.bits(),
            "transient-attachment",
        ),
    ]
    .into_iter()
    .filter_map(|(bit, label)| (flags.bits() & bit == bit).then_some(label))
    .collect()
}

fn memory_property_flag_labels(flags: u32) -> Vec<&'static str> {
    [
        (vk::MemoryPropertyFlags::DEVICE_LOCAL.bits(), "device-local"),
        (vk::MemoryPropertyFlags::HOST_VISIBLE.bits(), "host-visible"),
        (
            vk::MemoryPropertyFlags::HOST_COHERENT.bits(),
            "host-coherent",
        ),
        (vk::MemoryPropertyFlags::HOST_CACHED.bits(), "host-cached"),
    ]
    .into_iter()
    .filter_map(|(bit, label)| (flags & bit == bit).then_some(label))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_contiguous_mip_uploads() {
        let uploads = vec![
            NativeVulkanVulkanaliaImageMipUpload {
                buffer_offset: 0,
                byte_count: 16,
                width: 4,
                height: 4,
            },
            NativeVulkanVulkanaliaImageMipUpload {
                buffer_offset: 16,
                byte_count: 4,
                width: 2,
                height: 2,
            },
        ];

        validate_mip_uploads("test", 20, &uploads).expect("valid uploads");
    }

    #[test]
    fn rejects_gapped_mip_uploads() {
        let uploads = vec![NativeVulkanVulkanaliaImageMipUpload {
            buffer_offset: 4,
            byte_count: 16,
            width: 4,
            height: 4,
        }];

        let err = validate_mip_uploads("test", 20, &uploads).expect_err("gap");

        assert!(err.contains("expected 0"));
    }
}
