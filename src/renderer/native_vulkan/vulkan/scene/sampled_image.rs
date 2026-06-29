#![allow(dead_code)]

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::slice;
use std::sync::Once;

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

const SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES: u32 = 20;
const SCENE_FULL_SAMPLED_IMAGE_VERTEX_COUNT: usize = 4;
const SCENE_FULL_SAMPLED_IMAGE_INDEX_COUNT: usize = 6;
const DEVICE_LOCAL_MEMORY_FLAG_BITS: u32 = vk::MemoryPropertyFlags::DEVICE_LOCAL.bits();
const HOST_VISIBLE_MEMORY_FLAG_BITS: u32 = vk::MemoryPropertyFlags::HOST_VISIBLE.bits();
const HOST_VISIBLE_COHERENT_MEMORY_FLAG_BITS: u32 =
    vk::MemoryPropertyFlags::HOST_VISIBLE.bits() | vk::MemoryPropertyFlags::HOST_COHERENT.bits();
const GILDER_SCENE_TEXTURE_MAGIC: &[u8; 8] = b"GDTEX002";
const GILDER_SCENE_TEXTURE_HEADER_BYTES: usize = 32;
const GILDER_SCENE_TEXTURE_FORMAT_BC1_RGBA_UNORM_BLOCK: u32 = 1;
const GILDER_SCENE_TEXTURE_FORMAT_BC3_UNORM_BLOCK: u32 = 3;
const GILDER_SCENE_TEXTURE_FORMAT_BC7_UNORM_BLOCK: u32 = 7;
const SCENE_BC_BLOCK_TEXELS: u32 = 4;
const SCENE_BC1_BLOCK_BYTES: u64 = 8;
const SCENE_BC3_BLOCK_BYTES: u64 = 16;
const SCENE_BC7_BLOCK_BYTES: u64 = 16;
const SCENE_GTEX_UPLOAD_CHUNK_BYTES: usize = 128 * 1024;

use super::features::{
    NativeVulkanVulkanaliaCoreFeatureSnapshot,
    NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    NativeVulkanVulkanaliaVulkan14PropertySnapshot,
};
use super::memory::{
    native_vulkan_vulkanalia_bind_buffer_memory2, native_vulkan_vulkanalia_bind_image_memory2,
    native_vulkan_vulkanalia_map_memory2, native_vulkan_vulkanalia_unmap_memory2,
};
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
    pub payload_offset: u64,
    pub payload_len: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum NativeVulkanVulkanaliaSceneNativeTextureFormat {
    Bc1RgbaUnormBlock,
    Bc3UnormBlock,
    Bc7UnormBlock,
}

impl NativeVulkanVulkanaliaSceneNativeTextureFormat {
    fn from_gtex(value: u32) -> Option<Self> {
        match value {
            GILDER_SCENE_TEXTURE_FORMAT_BC1_RGBA_UNORM_BLOCK => Some(Self::Bc1RgbaUnormBlock),
            GILDER_SCENE_TEXTURE_FORMAT_BC3_UNORM_BLOCK => Some(Self::Bc3UnormBlock),
            GILDER_SCENE_TEXTURE_FORMAT_BC7_UNORM_BLOCK => Some(Self::Bc7UnormBlock),
            _ => None,
        }
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn vk_format(self) -> vk::Format {
        match self {
            Self::Bc1RgbaUnormBlock => vk::Format::BC1_RGBA_UNORM_BLOCK,
            Self::Bc3UnormBlock => vk::Format::BC3_UNORM_BLOCK,
            Self::Bc7UnormBlock => vk::Format::BC7_UNORM_BLOCK,
        }
    }

    pub(in crate::renderer::native_vulkan::vulkan) fn label(self) -> &'static str {
        match self {
            Self::Bc1RgbaUnormBlock => "BC1_RGBA_UNORM_BLOCK",
            Self::Bc3UnormBlock => "BC3_UNORM_BLOCK",
            Self::Bc7UnormBlock => "BC7_UNORM_BLOCK",
        }
    }

    fn block_bytes(self) -> u64 {
        match self {
            Self::Bc1RgbaUnormBlock => SCENE_BC1_BLOCK_BYTES,
            Self::Bc3UnormBlock => SCENE_BC3_BLOCK_BYTES,
            Self::Bc7UnormBlock => SCENE_BC7_BLOCK_BYTES,
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

pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneTransferImageResources {
    pub(in crate::renderer::native_vulkan::vulkan) image: vk::Image,
    pub(in crate::renderer::native_vulkan::vulkan) memory: vk::DeviceMemory,
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
        sampled_image_format: "BC1_RGBA/BC3/BC7_UNORM_BLOCK",
        sampled_image_usage: vec!["transfer-dst", "sampled"],
        staging_buffer_usage: vec!["transfer-src"],
        image_layout_flow: vec![
            "undefined",
            "transfer-dst-optimal",
            "shader-read-only-optimal",
        ],
        upload_model: "stream native .gtex BC block payload directly into a 128KiB mapped staging buffer, submit one copy chunk, unmap/trim, then repeat; no CPU payload Vec is retained",
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
    let mut file = File::open(source)
        .map_err(|err| format!("load scene native texture {}: {err}", source.display()))?;
    let file_len = file
        .metadata()
        .map_err(|err| format!("stat scene native texture {}: {err}", source.display()))?
        .len();
    if file_len < GILDER_SCENE_TEXTURE_HEADER_BYTES as u64 {
        return Err(format!(
            "scene sampled image runtime requires native .gtex texture {}; file is shorter than the header",
            source.display()
        ));
    }
    let mut header = [0u8; GILDER_SCENE_TEXTURE_HEADER_BYTES];
    file.read_exact(&mut header).map_err(|err| {
        format!(
            "read scene native texture header {}: {err}",
            source.display()
        )
    })?;
    if header.get(0..8) != Some(GILDER_SCENE_TEXTURE_MAGIC.as_slice()) {
        return Err(format!(
            "scene sampled image runtime requires native .gtex texture {}; runtime image decoding is disabled",
            source.display()
        ));
    }
    let width = read_scene_texture_u32(&header, 8).ok_or_else(|| {
        format!(
            "scene native texture {} has truncated width",
            source.display()
        )
    })?;
    let height = read_scene_texture_u32(&header, 12).ok_or_else(|| {
        format!(
            "scene native texture {} has truncated height",
            source.display()
        )
    })?;
    let format = read_scene_texture_u32(&header, 16).ok_or_else(|| {
        format!(
            "scene native texture {} has truncated format",
            source.display()
        )
    })?;
    let payload_len = read_scene_texture_u64(&header, 24).ok_or_else(|| {
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
                "scene native texture {} uses unsupported GPU format {format}; expected BC1_RGBA_UNORM_BLOCK, BC3_UNORM_BLOCK, or BC7_UNORM_BLOCK",
                source.display()
            )
        },
    )?;
    let expected_len = scene_texture_payload_byte_len(format, width, height)?;
    if payload_len != expected_len {
        return Err(format!(
            "scene native texture {} declares payload {payload_len} bytes, expected {expected_len}",
            source.display()
        ));
    }
    let expected_file_len = (GILDER_SCENE_TEXTURE_HEADER_BYTES as u64)
        .checked_add(expected_len)
        .ok_or_else(|| {
            format!(
                "scene native texture {} expected file length overflowed",
                source.display()
            )
        })?;
    if file_len != expected_file_len {
        return Err(format!(
            "scene native texture {} contains {} payload bytes, expected {expected_len}",
            source.display(),
            file_len.saturating_sub(GILDER_SCENE_TEXTURE_HEADER_BYTES as u64)
        ));
    }

    Ok(NativeVulkanVulkanaliaSceneNativeTexture {
        source: source.to_path_buf(),
        width,
        height,
        format,
        payload_offset: GILDER_SCENE_TEXTURE_HEADER_BYTES as u64,
        payload_len,
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
    command_pool: vk::CommandPool,
    queue: vk::Queue,
    sampler_mode: NativeVulkanVulkanaliaSceneSampledImageSamplerMode,
    source_label: impl Into<String>,
    texture: &NativeVulkanVulkanaliaSceneNativeTexture,
) -> Result<VulkanaliaSceneSampledImageResources, String> {
    let extent = vk::Extent2D {
        width: texture.width,
        height: texture.height,
    };
    validate_scene_texture_payload_len(extent, texture.format, texture.payload_len)?;
    let source_label = source_label.into();

    let image_usage = vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED;
    let image_extent = vk::Extent3D {
        width: extent.width,
        height: extent.height,
        depth: 1,
    };
    let image_format = texture.format.vk_format();
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
        let image_memory_type = sampled_image_memory_type_index_excluding(
            &memory_type_candidates,
            memory_requirements.memory_type_bits,
            DEVICE_LOCAL_MEMORY_FLAG_BITS,
            HOST_VISIBLE_MEMORY_FLAG_BITS,
        )
        .ok_or_else(|| {
            format!(
                "scene sampled image requires device-local non-host-visible memory for bits 0x{:08x}",
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

        let upload = upload_scene_sampled_image_staging_from_gtex(
            device,
            memory_properties,
            command_pool,
            queue,
            image,
            extent,
            texture,
            SceneSampledImageUploadFinalLayout::ShaderReadOnly,
        )?;

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
                texture_payload_bytes: texture.payload_len,
                decoded_rgba_payload_retained_after_upload: false,
                image_format: texture.format.label(),
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
                staging_buffer_bytes: upload.staging_buffer_bytes,
                selected_staging_memory_type_index: upload.staging_memory_type.index,
                selected_staging_memory_property_flags: memory_property_flag_labels(
                    upload.staging_memory_type.property_flags_bits,
                ),
                image_view_created: true,
                sampler_created: true,
                sampler_address_mode: sampler_mode.label(),
                descriptor_model: "VK_EXT_descriptor_heap",
                descriptor_type: "combined-image-sampler",
                descriptor_image_layout: "shader-read-only-optimal",
                upload_command_recorded: true,
                upload_submitted: true,
                upload_wait_model: "direct read .gtex chunk into mapped 128KiB staging buffer + cmd_copy_buffer_to_image2 + queue_submit2 fence wait; unmap/trim after each chunk and free staging before present",
                final_image_layout: "shader-read-only-optimal",
                command_order: sampled_image_resource_command_order().to_vec(),
                uses_synchronization2: true,
                uses_copy2: true,
                uses_host_image_copy: false,
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

#[allow(clippy::too_many_arguments)]
pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_create_scene_transfer_image_resources(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    command_pool: vk::CommandPool,
    queue: vk::Queue,
    source_label: impl Into<String>,
    texture: &NativeVulkanVulkanaliaSceneNativeTexture,
) -> Result<VulkanaliaSceneTransferImageResources, String> {
    let extent = vk::Extent2D {
        width: texture.width,
        height: texture.height,
    };
    validate_scene_texture_payload_len(extent, texture.format, texture.payload_len)?;
    let source_label = source_label.into();
    let image_usage = vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::TRANSFER_SRC;
    let image_extent = vk::Extent3D {
        width: extent.width,
        height: extent.height,
        depth: 1,
    };
    let image_format = texture.format.vk_format();
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
    let image = unsafe { device.create_image(&image_create_info, None) }
        .map_err(|err| format!("vkCreateImage(vulkanalia scene transfer image): {err:?}"))?;

    let mut image_live = true;
    let mut memory = vk::DeviceMemory::default();
    let mut memory_live = false;
    let result = (|| -> Result<VulkanaliaSceneTransferImageResources, String> {
        let memory_requirements = unsafe { device.get_image_memory_requirements(image) };
        let memory_type_candidates =
            native_vulkan_vulkanalia_memory_type_candidates(memory_properties);
        let image_memory_type = sampled_image_memory_type_index_excluding(
            &memory_type_candidates,
            memory_requirements.memory_type_bits,
            DEVICE_LOCAL_MEMORY_FLAG_BITS,
            HOST_VISIBLE_MEMORY_FLAG_BITS,
        )
        .ok_or_else(|| {
            format!(
                "scene transfer image requires device-local non-host-visible memory for bits 0x{:08x}",
                memory_requirements.memory_type_bits
            )
        })?;
        let allocation_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(memory_requirements.size)
            .memory_type_index(image_memory_type.index);
        memory = unsafe { device.allocate_memory(&allocation_info, None) }
            .map_err(|err| format!("vkAllocateMemory(vulkanalia scene transfer image): {err:?}"))?;
        memory_live = true;

        native_vulkan_vulkanalia_bind_image_memory2(
            device,
            image,
            memory,
            0,
            "scene transfer image",
        )?;

        let upload = upload_scene_sampled_image_staging_from_gtex(
            device,
            memory_properties,
            command_pool,
            queue,
            image,
            extent,
            texture,
            SceneSampledImageUploadFinalLayout::TransferSrc,
        )?;

        image_live = false;
        memory_live = false;

        Ok(VulkanaliaSceneTransferImageResources {
            image,
            memory,
            snapshot: NativeVulkanVulkanaliaSceneSampledImageResourceSnapshot {
                binding: "vulkanalia",
                route: "scene-transfer-image-first-present-resource",
                source_label,
                extent: (extent.width, extent.height),
                texture_payload_bytes: texture.payload_len,
                decoded_rgba_payload_retained_after_upload: false,
                image_format: texture.format.label(),
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
                staging_buffer_bytes: upload.staging_buffer_bytes,
                selected_staging_memory_type_index: upload.staging_memory_type.index,
                selected_staging_memory_property_flags: memory_property_flag_labels(
                    upload.staging_memory_type.property_flags_bits,
                ),
                image_view_created: false,
                sampler_created: false,
                sampler_address_mode: "none-transfer-only",
                descriptor_model: "none-transfer-only",
                descriptor_type: "none",
                descriptor_image_layout: "transfer-src-optimal",
                upload_command_recorded: true,
                upload_submitted: true,
                upload_wait_model: "direct read .gtex chunk into mapped 128KiB staging buffer + cmd_copy_buffer_to_image2 + queue_submit2 fence wait; final layout is transfer-src-optimal for first-present blit",
                final_image_layout: "transfer-src-optimal",
                command_order: transfer_image_resource_command_order().to_vec(),
                uses_synchronization2: true,
                uses_copy2: true,
                uses_host_image_copy: false,
                retained_across_present_frames: false,
            },
        })
    })();

    if result.is_err() {
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

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_destroy_scene_transfer_image_resources(
    device: &Device,
    resources: VulkanaliaSceneTransferImageResources,
) {
    unsafe {
        device.destroy_image(resources.image, None);
        device.free_memory(resources.memory, None);
    }
}

#[cfg(all(feature = "native-vulkan-video", target_os = "linux"))]
unsafe extern "C" {
    fn gilder_configure_process_allocator_for_streaming_video();
    fn gilder_trim_process_heap();
}

pub(crate) fn native_vulkan_vulkanalia_configure_scene_sampled_image_allocator() {
    #[cfg(all(feature = "native-vulkan-video", target_os = "linux"))]
    {
        static CONFIGURE_ALLOCATOR: Once = Once::new();
        CONFIGURE_ALLOCATOR.call_once(|| unsafe {
            gilder_configure_process_allocator_for_streaming_video();
        });
    }
}

pub(crate) fn native_vulkan_vulkanalia_trim_scene_sampled_image_decode_heap() {
    #[cfg(all(feature = "native-vulkan-video", target_os = "linux"))]
    unsafe {
        gilder_trim_process_heap();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SceneSampledImageUploadResult {
    staging_buffer_bytes: u64,
    staging_memory_type: NativeVulkanVulkanaliaMemoryTypeCandidate,
}

struct SceneSampledImageTransientStagingBuffer {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    memory_type: NativeVulkanVulkanaliaMemoryTypeCandidate,
    size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SceneSampledImageUploadFinalLayout {
    ShaderReadOnly,
    TransferSrc,
}

impl SceneSampledImageUploadFinalLayout {
    fn label(self) -> &'static str {
        match self {
            Self::ShaderReadOnly => "shader-read-only-optimal",
            Self::TransferSrc => "transfer-src-optimal",
        }
    }
}

fn upload_scene_sampled_image_staging_from_gtex(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    command_pool: vk::CommandPool,
    queue: vk::Queue,
    image: vk::Image,
    extent: vk::Extent2D,
    texture: &NativeVulkanVulkanaliaSceneNativeTexture,
    final_layout: SceneSampledImageUploadFinalLayout,
) -> Result<SceneSampledImageUploadResult, String> {
    let blocks_w = u64::from(extent.width.div_ceil(SCENE_BC_BLOCK_TEXELS));
    let blocks_h = u64::from(extent.height.div_ceil(SCENE_BC_BLOCK_TEXELS));
    let row_bytes = blocks_w
        .checked_mul(texture.format.block_bytes())
        .ok_or_else(|| {
            format!(
                "scene sampled image {} row byte count overflowed",
                texture.format.label()
            )
        })?;
    if row_bytes > SCENE_GTEX_UPLOAD_CHUNK_BYTES as u64 {
        return Err(format!(
            "scene sampled image {} row is {row_bytes} bytes; native runtime upload is capped at {SCENE_GTEX_UPLOAD_CHUNK_BYTES} bytes to avoid heap payload retention",
            texture.format.label()
        ));
    }
    let rows_per_chunk = ((SCENE_GTEX_UPLOAD_CHUNK_BYTES as u64) / row_bytes).max(1);
    let staging_buffer_bytes = texture
        .payload_len
        .min(SCENE_GTEX_UPLOAD_CHUNK_BYTES as u64)
        .max(row_bytes);
    let staging = create_scene_sampled_image_transient_staging_buffer(
        device,
        memory_properties,
        staging_buffer_bytes,
    )?;

    let command_buffer_info = vk::CommandBufferAllocateInfo::builder()
        .command_pool(command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1)
        .build();
    let command_buffers = match unsafe { device.allocate_command_buffers(&command_buffer_info) } {
        Ok(buffers) => buffers,
        Err(err) => {
            destroy_scene_sampled_image_transient_staging_buffer(device, staging);
            return Err(format!(
                "vkAllocateCommandBuffers(vulkanalia scene sampled image upload): {err:?}"
            ));
        }
    };
    let Some(command_buffer) = command_buffers.first().copied() else {
        destroy_scene_sampled_image_transient_staging_buffer(device, staging);
        return Err(
            "vkAllocateCommandBuffers(vulkanalia scene sampled image upload) returned none"
                .to_owned(),
        );
    };
    let fence_info = vk::FenceCreateInfo::builder();
    let fence = match unsafe { device.create_fence(&fence_info, None) } {
        Ok(fence) => fence,
        Err(err) => {
            unsafe {
                device.free_command_buffers(command_pool, &[command_buffer]);
            }
            destroy_scene_sampled_image_transient_staging_buffer(device, staging);
            return Err(format!(
                "vkCreateFence(vulkanalia scene sampled image upload): {err:?}"
            ));
        }
    };

    let result = upload_scene_sampled_image_staging_payload(
        device,
        queue,
        command_buffer,
        fence,
        &staging,
        image,
        extent,
        texture,
        row_bytes,
        rows_per_chunk,
        blocks_h,
        final_layout,
    );

    unsafe {
        device.destroy_fence(fence, None);
        device.free_command_buffers(command_pool, &[command_buffer]);
    }
    let upload = SceneSampledImageUploadResult {
        staging_buffer_bytes: staging.size_bytes,
        staging_memory_type: staging.memory_type,
    };
    destroy_scene_sampled_image_transient_staging_buffer(device, staging);
    result?;
    Ok(upload)
}

#[allow(clippy::too_many_arguments)]
fn upload_scene_sampled_image_staging_payload(
    device: &Device,
    queue: vk::Queue,
    command_buffer: vk::CommandBuffer,
    fence: vk::Fence,
    staging: &SceneSampledImageTransientStagingBuffer,
    image: vk::Image,
    extent: vk::Extent2D,
    texture: &NativeVulkanVulkanaliaSceneNativeTexture,
    row_bytes: u64,
    rows_per_chunk: u64,
    blocks_h: u64,
    final_layout: SceneSampledImageUploadFinalLayout,
) -> Result<(), String> {
    let mut file = File::open(&texture.source).map_err(|err| {
        format!(
            "open scene native texture payload {}: {err}",
            texture.source.display()
        )
    })?;
    file.seek(SeekFrom::Start(texture.payload_offset))
        .map_err(|err| {
            format!(
                "seek scene native texture payload {}: {err}",
                texture.source.display()
            )
        })?;
    let mut block_y = 0u64;
    let mut uploaded_bytes = 0u64;
    while block_y < blocks_h {
        let rows = rows_per_chunk.min(blocks_h - block_y);
        let chunk_bytes = rows
            .checked_mul(row_bytes)
            .ok_or_else(|| "scene sampled image upload chunk bytes overflowed".to_owned())?;
        let chunk_len = usize::try_from(chunk_bytes)
            .map_err(|_| "scene sampled image upload chunk does not fit usize".to_owned())?;
        read_scene_sampled_image_staging_chunk(
            device,
            staging,
            &mut file,
            &texture.source,
            texture.payload_offset + uploaded_bytes,
            chunk_len,
        )?;
        let first_chunk = block_y == 0;
        let last_chunk = block_y + rows >= blocks_h;
        record_submit_scene_sampled_image_staging_chunk(
            device,
            queue,
            command_buffer,
            fence,
            staging.buffer,
            image,
            extent,
            block_y,
            rows,
            first_chunk,
            last_chunk,
            final_layout,
        )?;
        block_y += rows;
        uploaded_bytes = uploaded_bytes
            .checked_add(chunk_bytes)
            .ok_or_else(|| "scene sampled image uploaded byte count overflowed".to_owned())?;
        native_vulkan_vulkanalia_trim_scene_sampled_image_decode_heap();
    }
    native_vulkan_vulkanalia_trim_scene_sampled_image_decode_heap();
    if uploaded_bytes != texture.payload_len {
        return Err(format!(
            "scene native texture {} streamed {uploaded_bytes} payload bytes, expected {}",
            texture.source.display(),
            texture.payload_len
        ));
    }
    Ok(())
}

fn create_scene_sampled_image_transient_staging_buffer(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    size_bytes: u64,
) -> Result<SceneSampledImageTransientStagingBuffer, String> {
    if size_bytes == 0 {
        return Err("scene sampled image staging buffer requires non-zero size".to_owned());
    }
    let create_info = vk::BufferCreateInfo::builder()
        .size(size_bytes)
        .usage(vk::BufferUsageFlags::TRANSFER_SRC)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .build();
    let buffer = unsafe { device.create_buffer(&create_info, None) }.map_err(|err| {
        format!("vkCreateBuffer(vulkanalia scene sampled image staging): {err:?}")
    })?;
    let result = (|| -> Result<SceneSampledImageTransientStagingBuffer, String> {
        let memory_requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
        let memory_type_candidates =
            native_vulkan_vulkanalia_memory_type_candidates(memory_properties);
        let memory_type = sampled_image_memory_type_index(
            &memory_type_candidates,
            memory_requirements.memory_type_bits,
            HOST_VISIBLE_COHERENT_MEMORY_FLAG_BITS,
        )
        .ok_or_else(|| {
            format!(
                "scene sampled image staging requires host-visible coherent memory for bits 0x{:08x}",
                memory_requirements.memory_type_bits
            )
        })?;
        let allocation_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(memory_requirements.size)
            .memory_type_index(memory_type.index)
            .build();
        let memory = unsafe { device.allocate_memory(&allocation_info, None) }.map_err(|err| {
            format!("vkAllocateMemory(vulkanalia scene sampled image staging): {err:?}")
        })?;

        if let Err(err) = native_vulkan_vulkanalia_bind_buffer_memory2(
            device,
            buffer,
            memory,
            0,
            "scene sampled image staging",
        ) {
            unsafe {
                device.free_memory(memory, None);
            }
            return Err(err);
        }

        Ok(SceneSampledImageTransientStagingBuffer {
            buffer,
            memory,
            memory_type,
            size_bytes,
        })
    })();

    if result.is_err() {
        unsafe {
            device.destroy_buffer(buffer, None);
        }
    }
    result
}

fn destroy_scene_sampled_image_transient_staging_buffer(
    device: &Device,
    staging: SceneSampledImageTransientStagingBuffer,
) {
    unsafe {
        device.destroy_buffer(staging.buffer, None);
        device.free_memory(staging.memory, None);
    }
}

fn read_scene_sampled_image_staging_chunk(
    device: &Device,
    staging: &SceneSampledImageTransientStagingBuffer,
    file: &mut File,
    source: &Path,
    source_offset: u64,
    chunk_len: usize,
) -> Result<(), String> {
    if chunk_len as u64 > staging.size_bytes {
        return Err(format!(
            "scene sampled image staging chunk {} bytes exceeds staging buffer {} bytes",
            chunk_len, staging.size_bytes
        ));
    }
    let map = native_vulkan_vulkanalia_map_memory2(
        device,
        staging.memory,
        0,
        chunk_len as u64,
        vk::MemoryMapFlags::empty(),
        "scene sampled image staging",
    )?;
    let read_result = {
        let mapped = unsafe { slice::from_raw_parts_mut(map.cast::<u8>(), chunk_len) };
        file.read_exact(mapped).map_err(|err| {
            format!(
                "read scene native texture payload {} at byte {}: {err}",
                source.display(),
                source_offset
            )
        })
    };
    let unmap_result = native_vulkan_vulkanalia_unmap_memory2(
        device,
        staging.memory,
        "scene sampled image staging",
    );
    read_result?;
    unmap_result
}

#[allow(clippy::too_many_arguments)]
fn record_submit_scene_sampled_image_staging_chunk(
    device: &Device,
    queue: vk::Queue,
    command_buffer: vk::CommandBuffer,
    fence: vk::Fence,
    staging_buffer: vk::Buffer,
    image: vk::Image,
    extent: vk::Extent2D,
    block_y: u64,
    block_rows: u64,
    first_chunk: bool,
    last_chunk: bool,
    final_layout: SceneSampledImageUploadFinalLayout,
) -> Result<(), String> {
    let y_offset = block_y
        .checked_mul(u64::from(SCENE_BC_BLOCK_TEXELS))
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| "scene sampled image chunk y offset overflowed".to_owned())?;
    let y_offset_u32 =
        u32::try_from(y_offset).map_err(|_| "scene sampled image chunk y offset is negative")?;
    let remaining_height = extent.height.saturating_sub(y_offset_u32);
    let copy_height = u32::try_from(
        block_rows
            .checked_mul(u64::from(SCENE_BC_BLOCK_TEXELS))
            .ok_or_else(|| "scene sampled image chunk height overflowed".to_owned())?,
    )
    .map_err(|_| "scene sampled image chunk height does not fit u32".to_owned())?
    .min(remaining_height);
    let image_subresource = vk::ImageSubresourceLayers::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .mip_level(0)
        .base_array_layer(0)
        .layer_count(1)
        .build();
    let copy = vk::BufferImageCopy2::builder()
        .buffer_offset(0)
        .buffer_row_length(0)
        .buffer_image_height(0)
        .image_subresource(image_subresource)
        .image_offset(vk::Offset3D {
            x: 0,
            y: y_offset,
            z: 0,
        })
        .image_extent(vk::Extent3D {
            width: extent.width,
            height: copy_height,
            depth: 1,
        })
        .build();
    let copies = [copy];
    let copy_info = vk::CopyBufferToImageInfo2::builder()
        .src_buffer(staging_buffer)
        .dst_image(image)
        .dst_image_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
        .regions(&copies)
        .build();

    unsafe {
        device
            .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
            .map_err(|err| {
                format!("vkResetCommandBuffer(vulkanalia scene sampled image upload): {err:?}")
            })?;
        let begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
            .build();
        device
            .begin_command_buffer(command_buffer, &begin_info)
            .map_err(|err| {
                format!("vkBeginCommandBuffer(vulkanalia scene sampled image upload): {err:?}")
            })?;

        if first_chunk {
            let to_transfer = vk::ImageMemoryBarrier2::builder()
                .src_stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE)
                .src_access_mask(vk::AccessFlags2::empty())
                .dst_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
                .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(scene_sampled_image_subresource_range())
                .build();
            let barriers = [to_transfer];
            let dependency = vk::DependencyInfo::builder()
                .image_memory_barriers(&barriers)
                .build();
            device.cmd_pipeline_barrier2(command_buffer, &dependency);
        }

        device.cmd_copy_buffer_to_image2(command_buffer, &copy_info);

        if last_chunk {
            let (dst_stage_mask, dst_access_mask, new_layout) = match final_layout {
                SceneSampledImageUploadFinalLayout::ShaderReadOnly => (
                    vk::PipelineStageFlags2::FRAGMENT_SHADER,
                    vk::AccessFlags2::SHADER_SAMPLED_READ,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                ),
                SceneSampledImageUploadFinalLayout::TransferSrc => (
                    vk::PipelineStageFlags2::ALL_TRANSFER,
                    vk::AccessFlags2::TRANSFER_READ,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                ),
            };
            let to_final_layout = vk::ImageMemoryBarrier2::builder()
                .src_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
                .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
                .dst_stage_mask(dst_stage_mask)
                .dst_access_mask(dst_access_mask)
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(new_layout)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(scene_sampled_image_subresource_range())
                .build();
            let barriers = [to_final_layout];
            let dependency = vk::DependencyInfo::builder()
                .image_memory_barriers(&barriers)
                .build();
            device.cmd_pipeline_barrier2(command_buffer, &dependency);
        }

        device.end_command_buffer(command_buffer).map_err(|err| {
            format!("vkEndCommandBuffer(vulkanalia scene sampled image upload): {err:?}")
        })?;

        let command_buffer_info = vk::CommandBufferSubmitInfo::builder()
            .command_buffer(command_buffer)
            .build();
        let command_buffer_infos = [command_buffer_info];
        let submit_info = vk::SubmitInfo2::builder()
            .command_buffer_infos(&command_buffer_infos)
            .build();
        device
            .queue_submit2(queue, &[submit_info], fence)
            .map_err(|err| {
                format!("vkQueueSubmit2(vulkanalia scene sampled image upload): {err:?}")
            })?;
        device
            .wait_for_fences(&[fence], true, u64::MAX)
            .map_err(|err| {
                format!("vkWaitForFences(vulkanalia scene sampled image upload): {err:?}")
            })?;
        device.reset_fences(&[fence]).map_err(|err| {
            format!("vkResetFences(vulkanalia scene sampled image upload): {err:?}")
        })?;
    }
    Ok(())
}

fn scene_sampled_image_command_order(backend_ready: bool) -> &'static [&'static str] {
    if backend_ready {
        &[
            "load_native_scene_texture_header_bc",
            "create_sampled_image_transfer_dst_sampled",
            "create_transient_staging_buffer_transfer_src",
            "stream_gtex_payload_block_rows",
            "cmd_pipeline_barrier2_transfer_dst",
            "cmd_copy_buffer_to_image2_chunks",
            "queue_submit2_upload_fence_wait",
            "cmd_pipeline_barrier2_shader_read",
            "destroy_transient_staging_buffer",
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
    validate_scene_texture_payload_len(extent, format, texture_bytes.len() as u64)
}

fn validate_scene_texture_payload_len(
    extent: vk::Extent2D,
    format: NativeVulkanVulkanaliaSceneNativeTextureFormat,
    texture_len: u64,
) -> Result<(), String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("scene sampled image upload requires non-zero extent".to_owned());
    }
    let expected = scene_texture_payload_byte_len(format, extent.width, extent.height)?;
    if texture_len != expected {
        return Err(format!(
            "scene sampled image upload expected {expected} {} bytes for {}x{}, got {}",
            format.label(),
            extent.width,
            extent.height,
            texture_len
        ));
    }
    Ok(())
}

fn scene_texture_payload_byte_len(
    format: NativeVulkanVulkanaliaSceneNativeTextureFormat,
    width: u32,
    height: u32,
) -> Result<u64, String> {
    let blocks_w = u64::from(width.div_ceil(SCENE_BC_BLOCK_TEXELS));
    let blocks_h = u64::from(height.div_ceil(SCENE_BC_BLOCK_TEXELS));
    blocks_w
        .checked_mul(blocks_h)
        .and_then(|blocks| blocks.checked_mul(format.block_bytes()))
        .ok_or_else(|| {
            format!(
                "scene sampled image extent {width}x{height} overflows {} byte size",
                format.label()
            )
        })
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
        "load_native_scene_texture_header_bc",
        "create_sampled_image_transfer_dst_sampled",
        "create_transient_staging_buffer_transfer_src",
        "stream_gtex_payload_block_rows",
        "cmd_pipeline_barrier2_transfer_dst",
        "cmd_copy_buffer_to_image2_chunks",
        "queue_submit2_upload_fence_wait",
        "cmd_pipeline_barrier2_shader_read",
        "destroy_transient_staging_buffer",
        "create_combined_image_sampler_descriptor",
        "retain_sampled_image_descriptor_for_present",
    ]
}

fn transfer_image_resource_command_order() -> &'static [&'static str] {
    &[
        "load_native_scene_texture_header_bc",
        "create_transfer_image_transfer_dst_transfer_src",
        "create_transient_staging_buffer_transfer_src",
        "stream_gtex_payload_block_rows",
        "cmd_pipeline_barrier2_transfer_dst",
        "cmd_copy_buffer_to_image2_chunks",
        "queue_submit2_upload_fence_wait",
        "cmd_pipeline_barrier2_transfer_src",
        "destroy_transient_staging_buffer",
        "retain_transfer_src_image_for_first_present",
    ]
}

fn sampled_image_memory_type_index_excluding(
    memory_types: &[NativeVulkanVulkanaliaMemoryTypeCandidate],
    allowed_memory_type_bits: u32,
    required_property_flags_bits: u32,
    excluded_property_flags_bits: u32,
) -> Option<NativeVulkanVulkanaliaMemoryTypeCandidate> {
    memory_types.iter().copied().find(|candidate| {
        let allowed = candidate.index < u32::BITS
            && allowed_memory_type_bits & (1u32 << candidate.index) != 0;
        let properties_match = candidate.property_flags_bits & required_property_flags_bits
            == required_property_flags_bits;
        let excluded_absent = candidate.property_flags_bits & excluded_property_flags_bits == 0;
        allowed && properties_match && excluded_absent
    })
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
    if flags.contains(vk::ImageUsageFlags::TRANSFER_DST) {
        labels.push("transfer-dst");
    }
    if flags.contains(vk::ImageUsageFlags::TRANSFER_SRC) {
        labels.push("transfer-src");
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
        assert_eq!(
            snapshot.sampled_image_format,
            "BC1_RGBA/BC3/BC7_UNORM_BLOCK"
        );
        assert_eq!(
            snapshot.sampled_image_usage,
            vec!["transfer-dst", "sampled"]
        );
        assert_eq!(snapshot.staging_buffer_usage, vec!["transfer-src"]);
        assert_eq!(
            snapshot.image_layout_flow,
            vec![
                "undefined",
                "transfer-dst-optimal",
                "shader-read-only-optimal"
            ]
        );
        assert!(
            snapshot
                .command_order
                .contains(&"stream_gtex_payload_block_rows")
        );
        assert!(
            snapshot
                .command_order
                .contains(&"cmd_copy_buffer_to_image2_chunks")
        );
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
    fn native_texture_upload_validation_matches_bc_formats() {
        let extent = vk::Extent2D {
            width: 8,
            height: 4,
        };

        assert_eq!(
            scene_texture_payload_byte_len(
                NativeVulkanVulkanaliaSceneNativeTextureFormat::Bc1RgbaUnormBlock,
                extent.width,
                extent.height
            ),
            Ok(16)
        );
        assert!(
            validate_scene_texture_upload(
                extent,
                NativeVulkanVulkanaliaSceneNativeTextureFormat::Bc1RgbaUnormBlock,
                &[0; 16],
            )
            .is_ok()
        );
        assert_eq!(
            scene_texture_payload_byte_len(
                NativeVulkanVulkanaliaSceneNativeTextureFormat::Bc3UnormBlock,
                extent.width,
                extent.height
            ),
            Ok(32)
        );
        assert!(
            validate_scene_texture_upload(
                extent,
                NativeVulkanVulkanaliaSceneNativeTextureFormat::Bc3UnormBlock,
                &[0; 32],
            )
            .is_ok()
        );
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
                property_flags_bits: (vk::MemoryPropertyFlags::DEVICE_LOCAL
                    | vk::MemoryPropertyFlags::HOST_VISIBLE)
                    .bits(),
            },
            NativeVulkanVulkanaliaMemoryTypeCandidate {
                index: 2,
                property_flags_bits: vk::MemoryPropertyFlags::DEVICE_LOCAL.bits(),
            },
        ];

        let device_local = sampled_image_memory_type_index_excluding(
            &memory_types,
            0b111,
            vk::MemoryPropertyFlags::DEVICE_LOCAL.bits(),
            vk::MemoryPropertyFlags::HOST_VISIBLE.bits(),
        )
        .expect("device-local memory type");
        assert_eq!(device_local.index, 2);

        let device_local_host_visible_rejected = sampled_image_memory_type_index_excluding(
            &memory_types,
            0b010,
            vk::MemoryPropertyFlags::DEVICE_LOCAL.bits(),
            vk::MemoryPropertyFlags::HOST_VISIBLE.bits(),
        );
        assert_eq!(device_local_host_visible_rejected, None);
    }

    #[test]
    fn sampled_image_resource_command_order_uploads_before_descriptor_present_reuse() {
        assert_eq!(
            sampled_image_resource_command_order(),
            &[
                "load_native_scene_texture_header_bc",
                "create_sampled_image_transfer_dst_sampled",
                "create_transient_staging_buffer_transfer_src",
                "stream_gtex_payload_block_rows",
                "cmd_pipeline_barrier2_transfer_dst",
                "cmd_copy_buffer_to_image2_chunks",
                "queue_submit2_upload_fence_wait",
                "cmd_pipeline_barrier2_shader_read",
                "destroy_transient_staging_buffer",
                "create_combined_image_sampler_descriptor",
                "retain_sampled_image_descriptor_for_present",
            ]
        );
    }

    #[test]
    fn loads_native_scene_gtex_bc_payloads() {
        assert_native_scene_gtex_loads(
            GILDER_SCENE_TEXTURE_FORMAT_BC1_RGBA_UNORM_BLOCK,
            NativeVulkanVulkanaliaSceneNativeTextureFormat::Bc1RgbaUnormBlock,
            8,
            4,
            16,
            "bc1",
        );
        assert_native_scene_gtex_loads(
            GILDER_SCENE_TEXTURE_FORMAT_BC3_UNORM_BLOCK,
            NativeVulkanVulkanaliaSceneNativeTextureFormat::Bc3UnormBlock,
            4,
            4,
            16,
            "bc3",
        );
        assert_native_scene_gtex_loads(
            GILDER_SCENE_TEXTURE_FORMAT_BC7_UNORM_BLOCK,
            NativeVulkanVulkanaliaSceneNativeTextureFormat::Bc7UnormBlock,
            4,
            4,
            16,
            "bc7",
        );
    }

    fn assert_native_scene_gtex_loads(
        gtex_format: u32,
        expected_format: NativeVulkanVulkanaliaSceneNativeTextureFormat,
        width: u32,
        height: u32,
        payload_len: usize,
        label: &str,
    ) {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "gilder-scene-{label}-texture-{}.gtex",
            std::process::id(),
        ));
        let payload = vec![7u8; payload_len];
        let mut bytes = Vec::new();
        bytes.extend_from_slice(GILDER_SCENE_TEXTURE_MAGIC);
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&height.to_le_bytes());
        bytes.extend_from_slice(&gtex_format.to_le_bytes());
        bytes.extend_from_slice(&1u32.to_le_bytes());
        bytes.extend_from_slice(&(payload.len() as u64).to_le_bytes());
        bytes.extend_from_slice(&payload);
        std::fs::write(&path, bytes).expect("write test gtex");

        let decoded = native_vulkan_vulkanalia_load_scene_native_texture(&path).expect("load gtex");
        let _ = std::fs::remove_file(&path);

        assert_eq!(decoded.width, width);
        assert_eq!(decoded.height, height);
        assert_eq!(decoded.format, expected_format);
        assert_eq!(
            decoded.payload_offset,
            GILDER_SCENE_TEXTURE_HEADER_BYTES as u64
        );
        assert_eq!(decoded.payload_len, payload.len() as u64);
    }
}
