#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

const SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES: u32 = 20;
const SCENE_FULL_SAMPLED_IMAGE_VERTEX_COUNT: usize = 4;
const SCENE_FULL_SAMPLED_IMAGE_INDEX_COUNT: usize = 6;
const DEVICE_LOCAL_MEMORY_FLAG_BITS: u32 = vk::MemoryPropertyFlags::DEVICE_LOCAL.bits();
const GILDER_SCENE_TEXTURE_MAGIC: &[u8; 8] = b"GDTEX002";
const GILDER_SCENE_TEXTURE_HEADER_BYTES: usize = 32;
const GILDER_SCENE_TEXTURE_FORMAT_BC7_UNORM_BLOCK: u32 = 7;
const SCENE_BC_BLOCK_TEXELS: u32 = 4;
const SCENE_BC7_BLOCK_BYTES: u64 = 16;

use super::features::{
    NativeVulkanVulkanaliaCoreFeatureSnapshot,
    NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    NativeVulkanVulkanaliaVulkan14PropertySnapshot,
};
use super::memory::native_vulkan_vulkanalia_bind_image_memory2;
use super::video_session::{
    NativeVulkanVulkanaliaMemoryTypeCandidate, native_vulkan_vulkanalia_memory_type_candidates,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanVulkanaliaSceneSampledImagePlanInput {
    pub sampled_image_sources: Vec<PathBuf>,
    pub recording_step_count: usize,
    pub vertex_count: usize,
    pub index_count: usize,
    pub vertex_buffer_bytes: u64,
    pub index_buffer_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSceneSampledImagePlanSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub backend_ready: bool,
    pub backend_status: &'static str,
    pub blocking_reason: Option<&'static str>,
    pub sampled_image_count: usize,
    pub resource_count: usize,
    pub sampled_image_sources: Vec<PathBuf>,
    pub recording_step_count: usize,
    pub vertex_count: usize,
    pub index_count: usize,
    pub vertex_buffer_bytes: u64,
    pub index_buffer_bytes: u64,
    pub vertex_stride_bytes: u32,
    pub descriptor_set_count: u32,
    pub descriptor_type: &'static str,
    pub descriptor_pool_combined_image_sampler_budget: u32,
    pub sampled_image_format: &'static str,
    pub sampled_image_usage: Vec<&'static str>,
    pub staging_buffer_usage: Vec<&'static str>,
    pub image_layout_flow: Vec<&'static str>,
    pub upload_model: &'static str,
    pub retains_decoded_rgba_payload_after_upload: bool,
    pub descriptor_model: &'static str,
    pub pipeline_label: &'static str,
    pub draw_indexed_count: u32,
    pub command_order: Vec<&'static str>,
    pub uses_pipeline_rendering_create_info: bool,
    pub uses_dynamic_rendering: bool,
    pub uses_synchronization2: bool,
    pub uses_submit2: bool,
    pub uses_push_descriptor_fast_path: bool,
    pub vulkan_1_4_push_descriptor_policy: &'static str,
    pub zero_copy_scope: &'static str,
    pub primary_reference: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSceneSampledImageDescriptorStrategySnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub sampled_image_count: usize,
    pub descriptor_set_path_enabled: bool,
    pub active_descriptor_model: &'static str,
    pub descriptor_heap_available: bool,
    pub descriptor_heap_fast_path_candidate: bool,
    pub uses_descriptor_heap_primary_path: bool,
    pub max_resource_heap_size: u64,
    pub image_descriptor_size: u64,
    pub sampler_descriptor_size: u64,
    pub push_descriptor_available: bool,
    pub max_push_descriptors: u32,
    pub push_descriptor_fast_path_candidate: bool,
    pub uses_push_descriptor_fast_path: bool,
    pub next_gate: &'static str,
    pub primary_reference: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSceneNativeTexture {
    pub source: PathBuf,
    pub width: u32,
    pub height: u32,
    pub format: NativeVulkanVulkanaliaSceneNativeTextureFormat,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum NativeVulkanVulkanaliaSceneNativeTextureFormat {
    Bc7UnormBlock,
}

impl NativeVulkanVulkanaliaSceneNativeTextureFormat {
    fn from_gtex(value: u32) -> Option<Self> {
        match value {
            GILDER_SCENE_TEXTURE_FORMAT_BC7_UNORM_BLOCK => Some(Self::Bc7UnormBlock),
            _ => None,
        }
    }

    fn vk_format(self) -> vk::Format {
        match self {
            Self::Bc7UnormBlock => vk::Format::BC7_UNORM_BLOCK,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Bc7UnormBlock => "BC7_UNORM_BLOCK",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSceneSampledImageResourceSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub source_label: String,
    pub extent: (u32, u32),
    pub texture_payload_bytes: u64,
    pub decoded_rgba_payload_retained_after_upload: bool,
    pub image_format: &'static str,
    pub image_usage: Vec<&'static str>,
    pub image_created: bool,
    pub image_memory_bound: bool,
    pub image_memory_size: u64,
    pub image_memory_alignment: u64,
    pub image_memory_type_bits: u32,
    pub selected_image_memory_type_index: u32,
    pub selected_image_memory_property_flags: Vec<&'static str>,
    pub staging_buffer_bytes: u64,
    pub selected_staging_memory_type_index: u32,
    pub selected_staging_memory_property_flags: Vec<&'static str>,
    pub image_view_created: bool,
    pub sampler_created: bool,
    pub sampler_address_mode: &'static str,
    pub descriptor_model: &'static str,
    pub descriptor_type: &'static str,
    pub descriptor_image_layout: &'static str,
    pub upload_command_recorded: bool,
    pub upload_submitted: bool,
    pub upload_wait_model: &'static str,
    pub final_image_layout: &'static str,
    pub command_order: Vec<&'static str>,
    pub uses_synchronization2: bool,
    pub uses_copy2: bool,
    pub uses_host_image_copy: bool,
    pub retained_across_present_frames: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::vulkan) enum NativeVulkanVulkanaliaSceneSampledImageSamplerMode
{
    ClampToEdge,
    Repeat,
}

impl NativeVulkanVulkanaliaSceneSampledImageSamplerMode {
    pub(in crate::renderer::native_vulkan::vulkan) fn address_mode(self) -> vk::SamplerAddressMode {
        match self {
            Self::ClampToEdge => vk::SamplerAddressMode::CLAMP_TO_EDGE,
            Self::Repeat => vk::SamplerAddressMode::REPEAT,
        }
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn label(self) -> &'static str {
        match self {
            Self::ClampToEdge => "clamp-to-edge",
            Self::Repeat => "repeat",
        }
    }
}

pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneSampledImageResources {
    pub(in crate::renderer::native_vulkan::vulkan) image: vk::Image,
    pub(in crate::renderer::native_vulkan::vulkan) memory: vk::DeviceMemory,
    pub(in crate::renderer::native_vulkan::vulkan) image_view_create_info: vk::ImageViewCreateInfo,
    pub(in crate::renderer::native_vulkan::vulkan) image_view: vk::ImageView,
    pub(in crate::renderer::native_vulkan::vulkan) sampler_create_info: vk::SamplerCreateInfo,
    pub(in crate::renderer::native_vulkan::vulkan) sampler: vk::Sampler,
    pub(in crate::renderer::native_vulkan::vulkan) snapshot:
        NativeVulkanVulkanaliaSceneSampledImageResourceSnapshot,
}

pub(crate) fn native_vulkan_vulkanalia_scene_sampled_image_plan(
    input: NativeVulkanVulkanaliaSceneSampledImagePlanInput,
) -> NativeVulkanVulkanaliaSceneSampledImagePlanSnapshot {
    let sampled_image_count = input.sampled_image_sources.len();
    let expected_vertex_count =
        sampled_image_count.saturating_mul(SCENE_FULL_SAMPLED_IMAGE_VERTEX_COUNT);
    let expected_index_count =
        sampled_image_count.saturating_mul(SCENE_FULL_SAMPLED_IMAGE_INDEX_COUNT);
    let backend_ready = sampled_image_count > 0
        && input.recording_step_count == sampled_image_count
        && input.vertex_count == expected_vertex_count
        && input.index_count == expected_index_count
        && input.vertex_buffer_bytes > 0
        && input.index_buffer_bytes > 0;
    let (backend_status, blocking_reason) = if backend_ready {
        ("sampled-image-dynamic-rendering-recording-ready", None)
    } else if sampled_image_count == 0 {
        ("no-sampled-image-quads", Some("no-sampled-image-quads"))
    } else {
        (
            "sampled-image-geometry-incomplete",
            Some("sampled-image-geometry-payload-incomplete"),
        )
    };
    let descriptor_budget = saturating_nonzero_u32(sampled_image_count);

    NativeVulkanVulkanaliaSceneSampledImagePlanSnapshot {
        binding: "vulkanalia",
        route: "scene-sampled-image-upload-descriptor-plan",
        backend_ready,
        backend_status,
        blocking_reason,
        sampled_image_count,
        resource_count: sampled_image_count,
        sampled_image_sources: input.sampled_image_sources,
        recording_step_count: input.recording_step_count,
        vertex_count: input.vertex_count,
        index_count: input.index_count,
        vertex_buffer_bytes: input.vertex_buffer_bytes,
        index_buffer_bytes: input.index_buffer_bytes,
        vertex_stride_bytes: SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES,
        descriptor_set_count: 0,
        descriptor_type: "combined-image-sampler",
        descriptor_pool_combined_image_sampler_budget: 0,
        sampled_image_format: "BC7_UNORM_BLOCK",
        sampled_image_usage: vec!["host-transfer", "transfer-dst", "sampled"],
        staging_buffer_usage: Vec::new(),
        image_layout_flow: vec![
            "undefined",
            "transfer-dst-optimal",
            "shader-read-only-optimal",
        ],
        upload_model: "load native .gtex BC7 GPU block texture payload, upload into retained sampled image with Vulkan 1.4 host image copy, reuse descriptor-heap records across present frames",
        retains_decoded_rgba_payload_after_upload: false,
        descriptor_model: "VK_EXT_descriptor_heap only; descriptor set and push descriptor paths are deleted",
        pipeline_label: "scene-sampled-image-alpha-blend",
        draw_indexed_count: if backend_ready { descriptor_budget } else { 0 },
        command_order: scene_sampled_image_command_order(backend_ready).to_vec(),
        uses_pipeline_rendering_create_info: backend_ready,
        uses_dynamic_rendering: backend_ready,
        uses_synchronization2: backend_ready,
        uses_submit2: backend_ready,
        uses_push_descriptor_fast_path: false,
        vulkan_1_4_push_descriptor_policy: "disabled for scene sampled images; VK_EXT_descriptor_heap is required",
        zero_copy_scope: "source image pixels upload once; present frames sample retained GPU image directly into the swapchain",
        primary_reference: "FFmpeg frame/descriptor lifetime discipline; Vulkan dynamic rendering and sync2 command ordering",
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_scene_sampled_image_descriptor_strategy(
    core_features: NativeVulkanVulkanaliaCoreFeatureSnapshot,
    vulkan_1_4_properties: NativeVulkanVulkanaliaVulkan14PropertySnapshot,
    descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    sampled_image_count: usize,
) -> NativeVulkanVulkanaliaSceneSampledImageDescriptorStrategySnapshot {
    let descriptor_heap_fast_path_candidate = core_features.descriptor_heap
        && sampled_image_count > 0
        && descriptor_heap_properties.max_resource_heap_size > 0
        && descriptor_heap_properties.image_descriptor_size > 0
        && descriptor_heap_properties.sampler_descriptor_size > 0;
    let push_descriptor_fast_path_candidate = false;

    NativeVulkanVulkanaliaSceneSampledImageDescriptorStrategySnapshot {
        binding: "vulkanalia",
        route: "scene-sampled-image-descriptor-strategy",
        sampled_image_count,
        descriptor_set_path_enabled: false,
        active_descriptor_model: if descriptor_heap_fast_path_candidate {
            "vulkan-ext-descriptor-heap-primary-path"
        } else {
            "descriptor-heap-required-unavailable"
        },
        descriptor_heap_available: core_features.descriptor_heap,
        descriptor_heap_fast_path_candidate,
        uses_descriptor_heap_primary_path: descriptor_heap_fast_path_candidate,
        max_resource_heap_size: descriptor_heap_properties.max_resource_heap_size,
        image_descriptor_size: descriptor_heap_properties.image_descriptor_size,
        sampler_descriptor_size: descriptor_heap_properties.sampler_descriptor_size,
        push_descriptor_available: core_features.push_descriptor,
        max_push_descriptors: vulkan_1_4_properties.max_push_descriptors,
        push_descriptor_fast_path_candidate,
        uses_push_descriptor_fast_path: false,
        next_gate: if descriptor_heap_fast_path_candidate {
            "heap-only scene sampled-image runtime coverage"
        } else {
            "require descriptor heap support for the retained sampled-image path"
        },
        primary_reference: "FFmpeg frame lifetime discipline; VK_EXT_descriptor_heap is the only descriptor model",
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_load_scene_native_texture(
    source: &Path,
) -> Result<NativeVulkanVulkanaliaSceneNativeTexture, String> {
    let mut bytes = fs::read(source)
        .map_err(|err| format!("load scene native texture {}: {err}", source.display()))?;
    if bytes.len() < GILDER_SCENE_TEXTURE_HEADER_BYTES {
        return Err(format!(
            "scene sampled image runtime requires native .gtex texture {}; file is shorter than the header",
            source.display()
        ));
    }
    if bytes.get(0..8) != Some(GILDER_SCENE_TEXTURE_MAGIC.as_slice()) {
        return Err(format!(
            "scene sampled image runtime requires native .gtex texture {}; runtime image decoding is disabled",
            source.display()
        ));
    }
    let width = read_scene_texture_u32(&bytes, 8).ok_or_else(|| {
        format!(
            "scene native texture {} has truncated width",
            source.display()
        )
    })?;
    let height = read_scene_texture_u32(&bytes, 12).ok_or_else(|| {
        format!(
            "scene native texture {} has truncated height",
            source.display()
        )
    })?;
    let format = read_scene_texture_u32(&bytes, 16).ok_or_else(|| {
        format!(
            "scene native texture {} has truncated format",
            source.display()
        )
    })?;
    let payload_len = read_scene_texture_u64(&bytes, 24).ok_or_else(|| {
        format!(
            "scene native texture {} has truncated payload length",
            source.display()
        )
    })?;
    if width == 0 || height == 0 {
        return Err(format!(
            "scene native texture {} declares zero extent",
            source.display()
        ));
    }
    let format = NativeVulkanVulkanaliaSceneNativeTextureFormat::from_gtex(format).ok_or_else(
        || {
            format!(
                "scene native texture {} uses unsupported GPU format {format}; expected BC7_UNORM_BLOCK",
                source.display()
            )
        },
    )?;
    if format != NativeVulkanVulkanaliaSceneNativeTextureFormat::Bc7UnormBlock {
        return Err(format!(
            "scene native texture {} uses unsupported GPU format {}; expected BC7_UNORM_BLOCK",
            source.display(),
            format.label()
        ));
    }
    let expected_len = scene_texture_payload_byte_len(format, width, height)?;
    if payload_len != expected_len {
        return Err(format!(
            "scene native texture {} declares payload {payload_len} bytes, expected {expected_len}",
            source.display()
        ));
    }
    let payload = bytes.split_off(GILDER_SCENE_TEXTURE_HEADER_BYTES);
    if payload.len() as u64 != expected_len {
        return Err(format!(
            "scene native texture {} contains {} payload bytes, expected {expected_len}",
            source.display(),
            payload.len()
        ));
    }

    Ok(NativeVulkanVulkanaliaSceneNativeTexture {
        source: source.to_path_buf(),
        width,
        height,
        format,
        bytes: payload,
    })
}

fn read_scene_texture_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

fn read_scene_texture_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}

#[allow(clippy::too_many_arguments)]
pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_create_scene_sampled_image_resources(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    _command_pool: vk::CommandPool,
    _queue: vk::Queue,
    sampler_mode: NativeVulkanVulkanaliaSceneSampledImageSamplerMode,
    source_label: impl Into<String>,
    extent: vk::Extent2D,
    texture_format: NativeVulkanVulkanaliaSceneNativeTextureFormat,
    texture_bytes: &[u8],
) -> Result<VulkanaliaSceneSampledImageResources, String> {
    validate_scene_texture_upload(extent, texture_format, texture_bytes)?;
    let source_label = source_label.into();

    let image_usage = vk::ImageUsageFlags::HOST_TRANSFER
        | vk::ImageUsageFlags::TRANSFER_DST
        | vk::ImageUsageFlags::SAMPLED;
    let image_extent = vk::Extent3D {
        width: extent.width,
        height: extent.height,
        depth: 1,
    };
    let image_format = texture_format.vk_format();
    let image_create_info = vk::ImageCreateInfo::builder()
        .image_type(vk::ImageType::_2D)
        .format(image_format)
        .extent(image_extent)
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(image_usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);
    let image = match unsafe { device.create_image(&image_create_info, None) } {
        Ok(image) => image,
        Err(err) => {
            return Err(format!(
                "vkCreateImage(vulkanalia scene sampled image): {err:?}"
            ));
        }
    };

    let mut image_live = true;
    let mut memory = vk::DeviceMemory::default();
    let mut memory_live = false;
    let mut image_view = vk::ImageView::default();
    let mut image_view_live = false;
    let mut sampler = vk::Sampler::default();
    let mut sampler_live = false;

    let result = (|| -> Result<VulkanaliaSceneSampledImageResources, String> {
        let memory_requirements = unsafe { device.get_image_memory_requirements(image) };
        let memory_type_candidates =
            native_vulkan_vulkanalia_memory_type_candidates(memory_properties);
        let image_memory_type = sampled_image_memory_type_index(
            &memory_type_candidates,
            memory_requirements.memory_type_bits,
            DEVICE_LOCAL_MEMORY_FLAG_BITS,
        )
        .or_else(|| {
            sampled_image_memory_type_index(
                &memory_type_candidates,
                memory_requirements.memory_type_bits,
                0,
            )
        })
        .ok_or_else(|| {
            format!(
                "scene sampled image has no compatible memory type for bits 0x{:08x}",
                memory_requirements.memory_type_bits
            )
        })?;
        let allocation_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(memory_requirements.size)
            .memory_type_index(image_memory_type.index);
        memory = unsafe { device.allocate_memory(&allocation_info, None) }
            .map_err(|err| format!("vkAllocateMemory(vulkanalia scene sampled image): {err:?}"))?;
        memory_live = true;

        native_vulkan_vulkanalia_bind_image_memory2(
            device,
            image,
            memory,
            0,
            "scene sampled image",
        )?;

        let image_view_info = scene_sampled_image_view_create_info(image, image_format);
        image_view = create_scene_sampled_image_view(device, &image_view_info)?;
        image_view_live = true;
        let sampler_info = scene_sampled_image_sampler_create_info(sampler_mode);
        sampler = create_scene_sampled_image_sampler(device, &sampler_info)?;
        sampler_live = true;

        upload_scene_sampled_image_host_copy(device, image, extent, texture_bytes)?;

        image_live = false;
        memory_live = false;
        image_view_live = false;
        sampler_live = false;

        Ok(VulkanaliaSceneSampledImageResources {
            image,
            memory,
            image_view_create_info: image_view_info,
            image_view,
            sampler_create_info: sampler_info,
            sampler,
            snapshot: NativeVulkanVulkanaliaSceneSampledImageResourceSnapshot {
                binding: "vulkanalia",
                route: "scene-sampled-image-retained-resource",
                source_label,
                extent: (extent.width, extent.height),
                texture_payload_bytes: texture_bytes.len() as u64,
                decoded_rgba_payload_retained_after_upload: false,
                image_format: texture_format.label(),
                image_usage: sampled_image_usage_labels(image_usage),
                image_created: true,
                image_memory_bound: true,
                image_memory_size: memory_requirements.size,
                image_memory_alignment: memory_requirements.alignment,
                image_memory_type_bits: memory_requirements.memory_type_bits,
                selected_image_memory_type_index: image_memory_type.index,
                selected_image_memory_property_flags: memory_property_flag_labels(
                    image_memory_type.property_flags_bits,
                ),
                staging_buffer_bytes: 0,
                selected_staging_memory_type_index: u32::MAX,
                selected_staging_memory_property_flags: Vec::new(),
                image_view_created: true,
                sampler_created: true,
                sampler_address_mode: sampler_mode.label(),
                descriptor_model: "VK_EXT_descriptor_heap",
                descriptor_type: "combined-image-sampler",
                descriptor_image_layout: "shader-read-only-optimal",
                upload_command_recorded: true,
                upload_submitted: false,
                upload_wait_model: "vkCopyMemoryToImage host copy + host image layout transitions; no queue submit or fence",
                final_image_layout: "shader-read-only-optimal",
                command_order: sampled_image_resource_command_order().to_vec(),
                uses_synchronization2: false,
                uses_copy2: false,
                uses_host_image_copy: true,
                retained_across_present_frames: true,
            },
        })
    })();

    if result.is_err() {
        if sampler_live {
            unsafe {
                device.destroy_sampler(sampler, None);
            }
        }
        if image_view_live {
            unsafe {
                device.destroy_image_view(image_view, None);
            }
        }
        if memory_live {
            unsafe {
                device.free_memory(memory, None);
            }
        }
        if image_live {
            unsafe {
                device.destroy_image(image, None);
            }
        }
    }

    result
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_destroy_scene_sampled_image_resources(
    device: &Device,
    resources: VulkanaliaSceneSampledImageResources,
) {
    unsafe {
        device.destroy_sampler(resources.sampler, None);
        device.destroy_image_view(resources.image_view, None);
        device.destroy_image(resources.image, None);
        device.free_memory(resources.memory, None);
    }
}

#[cfg(all(feature = "native-vulkan-video", target_os = "linux"))]
unsafe extern "C" {
    fn gilder_trim_process_heap();
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_trim_scene_sampled_image_decode_heap()
 {
    #[cfg(all(feature = "native-vulkan-video", target_os = "linux"))]
    unsafe {
        gilder_trim_process_heap();
    }
}

fn upload_scene_sampled_image_host_copy(
    device: &Device,
    image: vk::Image,
    extent: vk::Extent2D,
    texture_bytes: &[u8],
) -> Result<(), String> {
    let transfer_dst = vk::HostImageLayoutTransitionInfo::builder()
        .image(image)
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .subresource_range(scene_sampled_image_subresource_range())
        .build();
    unsafe { device.transition_image_layout(&[transfer_dst]) }.map_err(|err| {
        format!(
            "vkTransitionImageLayout(vulkanalia scene sampled image host transfer dst): {err:?}"
        )
    })?;

    let image_subresource = vk::ImageSubresourceLayers::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .mip_level(0)
        .base_array_layer(0)
        .layer_count(1)
        .build();
    let copy = vk::MemoryToImageCopy::builder()
        .host_pointer(&texture_bytes[0])
        .memory_row_length(0)
        .memory_image_height(0)
        .image_subresource(image_subresource)
        .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
        .image_extent(vk::Extent3D {
            width: extent.width,
            height: extent.height,
            depth: 1,
        })
        .build();
    let copies = [copy];
    let copy_info = vk::CopyMemoryToImageInfo::builder()
        .flags(vk::HostImageCopyFlags::empty())
        .dst_image(image)
        .dst_image_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .regions(&copies)
        .build();
    unsafe { device.copy_memory_to_image(&copy_info) }.map_err(|err| {
        format!("vkCopyMemoryToImage(vulkanalia scene sampled image host upload): {err:?}")
    })?;

    let shader_read = vk::HostImageLayoutTransitionInfo::builder()
        .image(image)
        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .subresource_range(scene_sampled_image_subresource_range())
        .build();
    unsafe { device.transition_image_layout(&[shader_read]) }.map_err(|err| {
        format!("vkTransitionImageLayout(vulkanalia scene sampled image shader read): {err:?}")
    })?;
    Ok(())
}

fn scene_sampled_image_command_order(backend_ready: bool) -> &'static [&'static str] {
    if backend_ready {
        &[
            "load_native_scene_texture_bc7",
            "create_sampled_image_host_transfer_sampled",
            "vk_transition_image_layout_transfer_dst",
            "vk_copy_memory_to_image",
            "vk_transition_image_layout_shader_read",
            "create_combined_image_sampler_descriptor",
            "cmd_begin_rendering",
            "cmd_bind_scene_sampled_image_pipeline",
            "cmd_bind_sampled_image_vertex_buffer",
            "cmd_bind_sampled_image_index_buffer",
            "cmd_bind_scene_descriptor_heap",
            "cmd_draw_indexed_per_image_quad",
            "cmd_end_rendering",
            "queue_submit2_present",
        ]
    } else {
        &["wait_for_scene_sampled_image_geometry"]
    }
}

fn saturating_nonzero_u32(value: usize) -> u32 {
    u32::try_from(value.max(1)).unwrap_or(u32::MAX)
}

fn validate_scene_texture_upload(
    extent: vk::Extent2D,
    format: NativeVulkanVulkanaliaSceneNativeTextureFormat,
    texture_bytes: &[u8],
) -> Result<(), String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("scene sampled image upload requires non-zero extent".to_owned());
    }
    let expected = scene_texture_payload_byte_len(format, extent.width, extent.height)?;
    if texture_bytes.len() as u64 != expected {
        return Err(format!(
            "scene sampled image upload expected {expected} {} bytes for {}x{}, got {}",
            format.label(),
            extent.width,
            extent.height,
            texture_bytes.len()
        ));
    }
    Ok(())
}

fn scene_texture_payload_byte_len(
    format: NativeVulkanVulkanaliaSceneNativeTextureFormat,
    width: u32,
    height: u32,
) -> Result<u64, String> {
    match format {
        NativeVulkanVulkanaliaSceneNativeTextureFormat::Bc7UnormBlock => {
            let blocks_w = u64::from(width.div_ceil(SCENE_BC_BLOCK_TEXELS));
            let blocks_h = u64::from(height.div_ceil(SCENE_BC_BLOCK_TEXELS));
            blocks_w
                .checked_mul(blocks_h)
                .and_then(|blocks| blocks.checked_mul(SCENE_BC7_BLOCK_BYTES))
                .ok_or_else(|| {
                    format!("scene sampled image extent {width}x{height} overflows BC7 byte size")
                })
        }
    }
}

fn scene_sampled_image_view_create_info(
    image: vk::Image,
    format: vk::Format,
) -> vk::ImageViewCreateInfo {
    let subresource_range = vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1)
        .build();
    vk::ImageViewCreateInfo::builder()
        .image(image)
        .view_type(vk::ImageViewType::_2D)
        .format(format)
        .subresource_range(subresource_range)
        .build()
}

fn create_scene_sampled_image_view(
    device: &Device,
    create_info: &vk::ImageViewCreateInfo,
) -> Result<vk::ImageView, String> {
    unsafe { device.create_image_view(create_info, None) }
        .map_err(|err| format!("vkCreateImageView(vulkanalia scene sampled image): {err:?}"))
}

fn scene_sampled_image_sampler_create_info(
    sampler_mode: NativeVulkanVulkanaliaSceneSampledImageSamplerMode,
) -> vk::SamplerCreateInfo {
    vk::SamplerCreateInfo::builder()
        .mag_filter(vk::Filter::LINEAR)
        .min_filter(vk::Filter::LINEAR)
        .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
        .address_mode_u(sampler_mode.address_mode())
        .address_mode_v(sampler_mode.address_mode())
        .address_mode_w(sampler_mode.address_mode())
        .min_lod(0.0)
        .max_lod(0.0)
        .build()
}

fn create_scene_sampled_image_sampler(
    device: &Device,
    sampler_info: &vk::SamplerCreateInfo,
) -> Result<vk::Sampler, String> {
    unsafe { device.create_sampler(sampler_info, None) }
        .map_err(|err| format!("vkCreateSampler(vulkanalia scene sampled image): {err:?}"))
}

fn scene_sampled_image_subresource_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1)
        .build()
}

fn sampled_image_resource_command_order() -> &'static [&'static str] {
    &[
        "load_native_scene_texture_bc7",
        "create_sampled_image_host_transfer_sampled",
        "vk_transition_image_layout_transfer_dst",
        "vk_copy_memory_to_image",
        "vk_transition_image_layout_shader_read",
        "create_combined_image_sampler_descriptor",
        "no_upload_queue_submit",
        "no_upload_fence_wait",
        "retain_sampled_image_descriptor_for_present",
    ]
}

fn sampled_image_memory_type_index(
    memory_types: &[NativeVulkanVulkanaliaMemoryTypeCandidate],
    allowed_memory_type_bits: u32,
    required_property_flags_bits: u32,
) -> Option<NativeVulkanVulkanaliaMemoryTypeCandidate> {
    memory_types.iter().copied().find(|candidate| {
        let allowed = candidate.index < u32::BITS
            && allowed_memory_type_bits & (1u32 << candidate.index) != 0;
        let properties_match = candidate.property_flags_bits & required_property_flags_bits
            == required_property_flags_bits;
        allowed && properties_match
    })
}

fn memory_property_flag_labels(bits: u32) -> Vec<&'static str> {
    [
        (vk::MemoryPropertyFlags::DEVICE_LOCAL.bits(), "device-local"),
        (vk::MemoryPropertyFlags::HOST_VISIBLE.bits(), "host-visible"),
        (
            vk::MemoryPropertyFlags::HOST_COHERENT.bits(),
            "host-coherent",
        ),
        (vk::MemoryPropertyFlags::HOST_CACHED.bits(), "host-cached"),
        (
            vk::MemoryPropertyFlags::LAZILY_ALLOCATED.bits(),
            "lazily-allocated",
        ),
        (vk::MemoryPropertyFlags::PROTECTED.bits(), "protected"),
    ]
    .into_iter()
    .filter_map(|(flag_bits, label)| (bits & flag_bits == flag_bits).then_some(label))
    .collect()
}

fn sampled_image_usage_labels(flags: vk::ImageUsageFlags) -> Vec<&'static str> {
    let mut labels = Vec::new();
    if flags.contains(vk::ImageUsageFlags::HOST_TRANSFER) {
        labels.push("host-transfer");
    }
    if flags.contains(vk::ImageUsageFlags::TRANSFER_DST) {
        labels.push("transfer-dst");
    }
    if flags.contains(vk::ImageUsageFlags::SAMPLED) {
        labels.push("sampled");
    }
    labels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampled_image_plan_marks_descriptor_upload_shape_ready() {
        let snapshot = native_vulkan_vulkanalia_scene_sampled_image_plan(
            NativeVulkanVulkanaliaSceneSampledImagePlanInput {
                sampled_image_sources: vec![PathBuf::from("/tmp/hero.png")],
                recording_step_count: 1,
                vertex_count: 4,
                index_count: 6,
                vertex_buffer_bytes: 80,
                index_buffer_bytes: 24,
            },
        );

        assert!(snapshot.backend_ready);
        assert_eq!(
            snapshot.backend_status,
            "sampled-image-dynamic-rendering-recording-ready"
        );
        assert_eq!(snapshot.blocking_reason, None);
        assert_eq!(snapshot.descriptor_set_count, 0);
        assert_eq!(snapshot.descriptor_type, "combined-image-sampler");
        assert_eq!(snapshot.sampled_image_format, "BC7_UNORM_BLOCK");
        assert_eq!(
            snapshot.sampled_image_usage,
            vec!["host-transfer", "transfer-dst", "sampled"]
        );
        assert!(snapshot.staging_buffer_usage.is_empty());
        assert_eq!(
            snapshot.image_layout_flow,
            vec![
                "undefined",
                "transfer-dst-optimal",
                "shader-read-only-optimal"
            ]
        );
        assert!(snapshot.command_order.contains(&"vk_copy_memory_to_image"));
        assert!(
            snapshot
                .command_order
                .contains(&"cmd_bind_scene_descriptor_heap")
        );
        assert_eq!(snapshot.draw_indexed_count, 1);
        assert!(!snapshot.retains_decoded_rgba_payload_after_upload);
        assert!(snapshot.uses_dynamic_rendering);
        assert!(snapshot.uses_synchronization2);
        assert!(snapshot.uses_submit2);
        assert!(!snapshot.uses_push_descriptor_fast_path);
    }

    #[test]
    fn sampled_image_plan_rejects_incomplete_geometry() {
        let snapshot = native_vulkan_vulkanalia_scene_sampled_image_plan(
            NativeVulkanVulkanaliaSceneSampledImagePlanInput {
                sampled_image_sources: vec![PathBuf::from("/tmp/hero.png")],
                recording_step_count: 1,
                vertex_count: 3,
                index_count: 6,
                vertex_buffer_bytes: 60,
                index_buffer_bytes: 24,
            },
        );

        assert!(!snapshot.backend_ready);
        assert_eq!(snapshot.backend_status, "sampled-image-geometry-incomplete");
        assert_eq!(
            snapshot.blocking_reason,
            Some("sampled-image-geometry-payload-incomplete")
        );
        assert_eq!(snapshot.descriptor_set_count, 0);
        assert_eq!(
            snapshot.command_order,
            vec!["wait_for_scene_sampled_image_geometry"]
        );
    }

    #[test]
    fn descriptor_strategy_requires_descriptor_heap_even_when_push_descriptor_exists() {
        let snapshot = native_vulkan_vulkanalia_scene_sampled_image_descriptor_strategy(
            NativeVulkanVulkanaliaCoreFeatureSnapshot {
                push_descriptor: true,
                ..NativeVulkanVulkanaliaCoreFeatureSnapshot::default()
            },
            NativeVulkanVulkanaliaVulkan14PropertySnapshot {
                max_push_descriptors: 8,
                ..NativeVulkanVulkanaliaVulkan14PropertySnapshot::default()
            },
            NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default(),
            2,
        );

        assert!(!snapshot.descriptor_set_path_enabled);
        assert!(!snapshot.descriptor_heap_fast_path_candidate);
        assert!(snapshot.push_descriptor_available);
        assert!(!snapshot.push_descriptor_fast_path_candidate);
        assert!(!snapshot.uses_push_descriptor_fast_path);
        assert_eq!(
            snapshot.active_descriptor_model,
            "descriptor-heap-required-unavailable"
        );
    }

    #[test]
    fn descriptor_strategy_prefers_descriptor_heap_over_push_descriptors() {
        let snapshot = native_vulkan_vulkanalia_scene_sampled_image_descriptor_strategy(
            NativeVulkanVulkanaliaCoreFeatureSnapshot {
                push_descriptor: true,
                descriptor_heap: true,
                ..NativeVulkanVulkanaliaCoreFeatureSnapshot::default()
            },
            NativeVulkanVulkanaliaVulkan14PropertySnapshot {
                max_push_descriptors: 8,
                ..NativeVulkanVulkanaliaVulkan14PropertySnapshot::default()
            },
            NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
                max_resource_heap_size: 4096,
                image_descriptor_size: 32,
                sampler_descriptor_size: 16,
                ..NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default()
            },
            2,
        );

        assert!(snapshot.descriptor_heap_available);
        assert!(snapshot.descriptor_heap_fast_path_candidate);
        assert!(snapshot.uses_descriptor_heap_primary_path);
        assert!(!snapshot.push_descriptor_fast_path_candidate);
        assert!(!snapshot.uses_push_descriptor_fast_path);
        assert_eq!(
            snapshot.active_descriptor_model,
            "vulkan-ext-descriptor-heap-primary-path"
        );
    }

    #[test]
    fn descriptor_strategy_rejects_push_descriptor_budget_without_heap() {
        let snapshot = native_vulkan_vulkanalia_scene_sampled_image_descriptor_strategy(
            NativeVulkanVulkanaliaCoreFeatureSnapshot {
                push_descriptor: true,
                ..NativeVulkanVulkanaliaCoreFeatureSnapshot::default()
            },
            NativeVulkanVulkanaliaVulkan14PropertySnapshot {
                max_push_descriptors: 1,
                ..NativeVulkanVulkanaliaVulkan14PropertySnapshot::default()
            },
            NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default(),
            64,
        );

        assert_eq!(snapshot.sampled_image_count, 64);
        assert!(!snapshot.push_descriptor_fast_path_candidate);
        assert!(!snapshot.uses_push_descriptor_fast_path);
        assert_eq!(
            snapshot.active_descriptor_model,
            "descriptor-heap-required-unavailable"
        );
    }

    #[test]
    fn native_texture_upload_validation_matches_bc7_extent() {
        let extent = vk::Extent2D {
            width: 8,
            height: 4,
        };

        assert_eq!(
            scene_texture_payload_byte_len(
                NativeVulkanVulkanaliaSceneNativeTextureFormat::Bc7UnormBlock,
                extent.width,
                extent.height
            ),
            Ok(32)
        );
        assert!(
            validate_scene_texture_upload(
                extent,
                NativeVulkanVulkanaliaSceneNativeTextureFormat::Bc7UnormBlock,
                &[0; 32],
            )
            .is_ok()
        );
        assert!(
            validate_scene_texture_upload(
                extent,
                NativeVulkanVulkanaliaSceneNativeTextureFormat::Bc7UnormBlock,
                &[0; 31],
            )
            .is_err()
        );
        assert!(
            validate_scene_texture_upload(
                vk::Extent2D {
                    width: 0,
                    height: 2
                },
                NativeVulkanVulkanaliaSceneNativeTextureFormat::Bc7UnormBlock,
                &[],
            )
            .is_err()
        );
    }

    #[test]
    fn sampled_image_sampler_modes_name_vulkan_address_modes() {
        assert_eq!(
            NativeVulkanVulkanaliaSceneSampledImageSamplerMode::ClampToEdge.address_mode(),
            vk::SamplerAddressMode::CLAMP_TO_EDGE
        );
        assert_eq!(
            NativeVulkanVulkanaliaSceneSampledImageSamplerMode::ClampToEdge.label(),
            "clamp-to-edge"
        );
        assert_eq!(
            NativeVulkanVulkanaliaSceneSampledImageSamplerMode::Repeat.address_mode(),
            vk::SamplerAddressMode::REPEAT
        );
        assert_eq!(
            NativeVulkanVulkanaliaSceneSampledImageSamplerMode::Repeat.label(),
            "repeat"
        );
    }

    #[test]
    fn sampled_image_memory_type_selection_respects_required_flags() {
        let memory_types = [
            NativeVulkanVulkanaliaMemoryTypeCandidate {
                index: 0,
                property_flags_bits: vk::MemoryPropertyFlags::HOST_VISIBLE.bits(),
            },
            NativeVulkanVulkanaliaMemoryTypeCandidate {
                index: 1,
                property_flags_bits: vk::MemoryPropertyFlags::DEVICE_LOCAL.bits(),
            },
        ];

        let device_local = sampled_image_memory_type_index(
            &memory_types,
            0b11,
            vk::MemoryPropertyFlags::DEVICE_LOCAL.bits(),
        )
        .expect("device-local memory type");
        assert_eq!(device_local.index, 1);

        let host_visible = sampled_image_memory_type_index(
            &memory_types,
            0b01,
            vk::MemoryPropertyFlags::HOST_VISIBLE.bits(),
        )
        .expect("host-visible memory type");
        assert_eq!(host_visible.index, 0);
    }

    #[test]
    fn sampled_image_resource_command_order_uploads_before_descriptor_present_reuse() {
        assert_eq!(
            sampled_image_resource_command_order(),
            &[
                "load_native_scene_texture_bc7",
                "create_sampled_image_host_transfer_sampled",
                "vk_transition_image_layout_transfer_dst",
                "vk_copy_memory_to_image",
                "vk_transition_image_layout_shader_read",
                "create_combined_image_sampler_descriptor",
                "no_upload_queue_submit",
                "no_upload_fence_wait",
                "retain_sampled_image_descriptor_for_present",
            ]
        );
    }

    #[test]
    fn loads_native_scene_gtex_bc7_payload() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "gilder-scene-bc7-texture-{}.gtex",
            std::process::id()
        ));
        let payload = [7u8; 16];
        let mut bytes = Vec::new();
        bytes.extend_from_slice(GILDER_SCENE_TEXTURE_MAGIC);
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&4u32.to_le_bytes());
        bytes.extend_from_slice(&GILDER_SCENE_TEXTURE_FORMAT_BC7_UNORM_BLOCK.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&payload);
        std::fs::write(&path, bytes).expect("write test gtex");

        let decoded = native_vulkan_vulkanalia_load_scene_native_texture(&path).expect("load gtex");
        let _ = std::fs::remove_file(&path);

        assert_eq!(decoded.width, 4);
        assert_eq!(decoded.height, 4);
        assert_eq!(
            decoded.format,
            NativeVulkanVulkanaliaSceneNativeTextureFormat::Bc7UnormBlock
        );
        assert_eq!(decoded.bytes, payload);
    }
}
