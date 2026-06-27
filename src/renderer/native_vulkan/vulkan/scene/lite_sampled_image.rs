#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::ptr;

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

const SCENE_LITE_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES: u32 = 20;
const SCENE_LITE_SAMPLED_IMAGE_VERTEX_COUNT: usize = 4;
const SCENE_LITE_SAMPLED_IMAGE_INDEX_COUNT: usize = 6;
const SCENE_LITE_RGBA_BYTES_PER_PIXEL: u64 = 4;
const DEVICE_LOCAL_MEMORY_FLAG_BITS: u32 = vk::MemoryPropertyFlags::DEVICE_LOCAL.bits();
const HOST_VISIBLE_COHERENT_MEMORY_FLAG_BITS: u32 =
    vk::MemoryPropertyFlags::HOST_VISIBLE.bits() | vk::MemoryPropertyFlags::HOST_COHERENT.bits();
const HOST_VISIBLE_MEMORY_FLAG_BITS: u32 = vk::MemoryPropertyFlags::HOST_VISIBLE.bits();

use super::features::{
    NativeVulkanVulkanaliaCoreFeatureSnapshot,
    NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    NativeVulkanVulkanaliaVulkan14PropertySnapshot,
};
use super::video_session::{
    NativeVulkanVulkanaliaMemoryTypeCandidate, native_vulkan_vulkanalia_memory_type_candidates,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeVulkanVulkanaliaSceneLiteSampledImagePlanInput {
    pub sampled_image_sources: Vec<PathBuf>,
    pub recording_step_count: usize,
    pub vertex_count: usize,
    pub index_count: usize,
    pub vertex_buffer_bytes: u64,
    pub index_buffer_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSceneLiteSampledImagePlanSnapshot {
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
pub struct NativeVulkanVulkanaliaSceneLiteSampledImageDescriptorStrategySnapshot {
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
pub struct NativeVulkanVulkanaliaSceneLiteDecodedRgbaImage {
    pub source: PathBuf,
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSceneLiteSampledImageResourceSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub source_label: String,
    pub extent: (u32, u32),
    pub rgba_bytes: u64,
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
    pub retained_across_present_frames: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::vulkan) enum NativeVulkanVulkanaliaSceneLiteSampledImageSamplerMode
{
    ClampToEdge,
    Repeat,
}

impl NativeVulkanVulkanaliaSceneLiteSampledImageSamplerMode {
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

pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneLiteSampledImageResources {
    pub(in crate::renderer::native_vulkan::vulkan) image: vk::Image,
    pub(in crate::renderer::native_vulkan::vulkan) memory: vk::DeviceMemory,
    pub(in crate::renderer::native_vulkan::vulkan) image_view_create_info: vk::ImageViewCreateInfo,
    pub(in crate::renderer::native_vulkan::vulkan) image_view: vk::ImageView,
    pub(in crate::renderer::native_vulkan::vulkan) sampler_create_info: vk::SamplerCreateInfo,
    pub(in crate::renderer::native_vulkan::vulkan) sampler: vk::Sampler,
    pub(in crate::renderer::native_vulkan::vulkan) snapshot:
        NativeVulkanVulkanaliaSceneLiteSampledImageResourceSnapshot,
}

struct VulkanaliaSceneLiteSampledImageUploadBuffer {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    memory_type: NativeVulkanVulkanaliaMemoryTypeCandidate,
    size: u64,
}

pub(crate) fn native_vulkan_vulkanalia_scene_lite_sampled_image_plan(
    input: NativeVulkanVulkanaliaSceneLiteSampledImagePlanInput,
) -> NativeVulkanVulkanaliaSceneLiteSampledImagePlanSnapshot {
    let sampled_image_count = input.sampled_image_sources.len();
    let expected_vertex_count =
        sampled_image_count.saturating_mul(SCENE_LITE_SAMPLED_IMAGE_VERTEX_COUNT);
    let expected_index_count =
        sampled_image_count.saturating_mul(SCENE_LITE_SAMPLED_IMAGE_INDEX_COUNT);
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

    NativeVulkanVulkanaliaSceneLiteSampledImagePlanSnapshot {
        binding: "vulkanalia",
        route: "scene-lite-sampled-image-upload-descriptor-plan",
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
        vertex_stride_bytes: SCENE_LITE_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES,
        descriptor_set_count: 0,
        descriptor_type: "combined-image-sampler",
        descriptor_pool_combined_image_sampler_budget: 0,
        sampled_image_format: "R8G8B8A8_UNORM",
        sampled_image_usage: vec!["transfer-dst", "sampled"],
        staging_buffer_usage: vec!["transfer-src"],
        image_layout_flow: vec![
            "undefined",
            "transfer-dst-optimal",
            "shader-read-only-optimal",
        ],
        upload_model: "decode source image to RGBA once, upload into retained sampled image, reuse descriptor-heap records across present frames",
        descriptor_model: "VK_EXT_descriptor_heap only; descriptor set and push descriptor fallbacks are disabled",
        pipeline_label: "scene-lite-sampled-image-alpha-blend",
        draw_indexed_count: if backend_ready { descriptor_budget } else { 0 },
        command_order: scene_lite_sampled_image_command_order(backend_ready).to_vec(),
        uses_pipeline_rendering_create_info: backend_ready,
        uses_dynamic_rendering: backend_ready,
        uses_synchronization2: backend_ready,
        uses_submit2: backend_ready,
        uses_push_descriptor_fast_path: false,
        vulkan_1_4_push_descriptor_policy: "disabled for scene-lite sampled images; VK_EXT_descriptor_heap is required",
        zero_copy_scope: "source image pixels upload once; present frames sample retained GPU image directly into the swapchain",
        primary_reference: "FFmpeg frame/descriptor lifetime discipline; Vulkan dynamic rendering and sync2 command ordering",
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_scene_lite_sampled_image_descriptor_strategy(
    core_features: NativeVulkanVulkanaliaCoreFeatureSnapshot,
    vulkan_1_4_properties: NativeVulkanVulkanaliaVulkan14PropertySnapshot,
    descriptor_heap_properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    sampled_image_count: usize,
) -> NativeVulkanVulkanaliaSceneLiteSampledImageDescriptorStrategySnapshot {
    let descriptor_heap_fast_path_candidate = core_features.descriptor_heap
        && sampled_image_count > 0
        && descriptor_heap_properties.max_resource_heap_size > 0
        && descriptor_heap_properties.image_descriptor_size > 0
        && descriptor_heap_properties.sampler_descriptor_size > 0;
    let push_descriptor_fast_path_candidate = false;

    NativeVulkanVulkanaliaSceneLiteSampledImageDescriptorStrategySnapshot {
        binding: "vulkanalia",
        route: "scene-lite-sampled-image-descriptor-strategy",
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

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_decode_scene_lite_rgba_image(
    source: &Path,
) -> Result<NativeVulkanVulkanaliaSceneLiteDecodedRgbaImage, String> {
    let image = image::open(source)
        .map_err(|err| {
            format!(
                "decode scene-lite sampled image {}: {err}",
                source.display()
            )
        })?
        .to_rgba8();
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return Err(format!(
            "scene-lite sampled image {} decoded to zero extent",
            source.display()
        ));
    }

    Ok(NativeVulkanVulkanaliaSceneLiteDecodedRgbaImage {
        source: source.to_path_buf(),
        width,
        height,
        bytes: image.into_raw(),
    })
}

#[allow(clippy::too_many_arguments)]
pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_create_scene_lite_sampled_image_resources(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    command_pool: vk::CommandPool,
    queue: vk::Queue,
    sampler_mode: NativeVulkanVulkanaliaSceneLiteSampledImageSamplerMode,
    source_label: impl Into<String>,
    extent: vk::Extent2D,
    rgba_bytes: &[u8],
) -> Result<VulkanaliaSceneLiteSampledImageResources, String> {
    validate_scene_lite_rgba_upload(extent, rgba_bytes)?;
    let source_label = source_label.into();
    let mut staging = Some(create_scene_lite_sampled_image_upload_buffer(
        device,
        memory_properties,
        rgba_bytes,
        vk::BufferUsageFlags::TRANSFER_SRC,
    )?);

    let image_usage = vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED;
    let image_extent = vk::Extent3D {
        width: extent.width,
        height: extent.height,
        depth: 1,
    };
    let image_create_info = vk::ImageCreateInfo::builder()
        .image_type(vk::ImageType::_2D)
        .format(vk::Format::R8G8B8A8_UNORM)
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
            if let Some(staging) = staging.take() {
                destroy_scene_lite_sampled_image_upload_buffer(device, staging);
            }
            return Err(format!(
                "vkCreateImage(vulkanalia scene-lite sampled image): {err:?}"
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

    let result = (|| -> Result<VulkanaliaSceneLiteSampledImageResources, String> {
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
                "scene-lite sampled image has no compatible memory type for bits 0x{:08x}",
                memory_requirements.memory_type_bits
            )
        })?;
        let allocation_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(memory_requirements.size)
            .memory_type_index(image_memory_type.index);
        memory = unsafe { device.allocate_memory(&allocation_info, None) }.map_err(|err| {
            format!("vkAllocateMemory(vulkanalia scene-lite sampled image): {err:?}")
        })?;
        memory_live = true;

        unsafe { device.bind_image_memory(image, memory, 0) }.map_err(|err| {
            format!("vkBindImageMemory(vulkanalia scene-lite sampled image): {err:?}")
        })?;

        let image_view_info = scene_lite_sampled_image_view_create_info(image);
        image_view = create_scene_lite_sampled_image_view(device, &image_view_info)?;
        image_view_live = true;
        let sampler_info = scene_lite_sampled_image_sampler_create_info(sampler_mode);
        sampler = create_scene_lite_sampled_image_sampler(device, &sampler_info)?;
        sampler_live = true;

        let staging_ref = staging
            .as_ref()
            .expect("scene-lite sampled image staging buffer is live during upload");
        upload_scene_lite_sampled_image(
            device,
            command_pool,
            queue,
            staging_ref.buffer,
            image,
            extent,
        )?;
        let staging = staging
            .take()
            .expect("scene-lite sampled image staging buffer is live after upload");
        let staging_memory_type = staging.memory_type;
        let staging_size = staging.size;
        destroy_scene_lite_sampled_image_upload_buffer(device, staging);

        image_live = false;
        memory_live = false;
        image_view_live = false;
        sampler_live = false;

        Ok(VulkanaliaSceneLiteSampledImageResources {
            image,
            memory,
            image_view_create_info: image_view_info,
            image_view,
            sampler_create_info: sampler_info,
            sampler,
            snapshot: NativeVulkanVulkanaliaSceneLiteSampledImageResourceSnapshot {
                binding: "vulkanalia",
                route: "scene-lite-sampled-image-retained-resource",
                source_label,
                extent: (extent.width, extent.height),
                rgba_bytes: rgba_bytes.len() as u64,
                image_format: "R8G8B8A8_UNORM",
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
                staging_buffer_bytes: staging_size,
                selected_staging_memory_type_index: staging_memory_type.index,
                selected_staging_memory_property_flags: memory_property_flag_labels(
                    staging_memory_type.property_flags_bits,
                ),
                image_view_created: true,
                sampler_created: true,
                sampler_address_mode: sampler_mode.label(),
                descriptor_model: "VK_EXT_descriptor_heap",
                descriptor_type: "combined-image-sampler",
                descriptor_image_layout: "shader-read-only-optimal",
                upload_command_recorded: true,
                upload_submitted: true,
                upload_wait_model: "queue_submit2 + fence wait during retained resource upload",
                final_image_layout: "shader-read-only-optimal",
                command_order: sampled_image_resource_command_order().to_vec(),
                uses_synchronization2: true,
                retained_across_present_frames: true,
            },
        })
    })();

    if result.is_err() {
        if let Some(staging) = staging.take() {
            destroy_scene_lite_sampled_image_upload_buffer(device, staging);
        }
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

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_destroy_scene_lite_sampled_image_resources(
    device: &Device,
    resources: VulkanaliaSceneLiteSampledImageResources,
) {
    unsafe {
        device.destroy_sampler(resources.sampler, None);
        device.destroy_image_view(resources.image_view, None);
        device.destroy_image(resources.image, None);
        device.free_memory(resources.memory, None);
    }
}

fn scene_lite_sampled_image_command_order(backend_ready: bool) -> &'static [&'static str] {
    if backend_ready {
        &[
            "decode_source_image_rgba",
            "create_sampled_image_transfer_dst_sampled",
            "create_rgba_upload_staging_buffer",
            "cmd_pipeline_barrier2_transfer_dst",
            "cmd_copy_buffer_to_image",
            "cmd_pipeline_barrier2_shader_read",
            "create_combined_image_sampler_descriptor",
            "cmd_begin_rendering",
            "cmd_bind_scene_lite_sampled_image_pipeline",
            "cmd_bind_sampled_image_vertex_buffer",
            "cmd_bind_sampled_image_index_buffer",
            "cmd_bind_scene_lite_descriptor_heap",
            "cmd_draw_indexed_per_image_quad",
            "cmd_end_rendering",
            "queue_submit2_present",
        ]
    } else {
        &["wait_for_scene_lite_sampled_image_geometry"]
    }
}

fn saturating_nonzero_u32(value: usize) -> u32 {
    u32::try_from(value.max(1)).unwrap_or(u32::MAX)
}

fn validate_scene_lite_rgba_upload(extent: vk::Extent2D, rgba_bytes: &[u8]) -> Result<(), String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("scene-lite sampled image upload requires non-zero extent".to_owned());
    }
    let expected = scene_lite_rgba_byte_len(extent)?;
    if rgba_bytes.len() as u64 != expected {
        return Err(format!(
            "scene-lite sampled image upload expected {expected} RGBA bytes for {}x{}, got {}",
            extent.width,
            extent.height,
            rgba_bytes.len()
        ));
    }
    Ok(())
}

fn scene_lite_rgba_byte_len(extent: vk::Extent2D) -> Result<u64, String> {
    u64::from(extent.width)
        .checked_mul(u64::from(extent.height))
        .and_then(|pixels| pixels.checked_mul(SCENE_LITE_RGBA_BYTES_PER_PIXEL))
        .ok_or_else(|| {
            format!(
                "scene-lite sampled image extent {}x{} overflows RGBA byte size",
                extent.width, extent.height
            )
        })
}

fn create_scene_lite_sampled_image_upload_buffer(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    payload: &[u8],
    usage: vk::BufferUsageFlags,
) -> Result<VulkanaliaSceneLiteSampledImageUploadBuffer, String> {
    let create_info = vk::BufferCreateInfo::builder()
        .size(payload.len() as u64)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let buffer = unsafe { device.create_buffer(&create_info, None) }.map_err(|err| {
        format!("vkCreateBuffer(vulkanalia scene-lite sampled image staging): {err:?}")
    })?;

    let result = (|| -> Result<VulkanaliaSceneLiteSampledImageUploadBuffer, String> {
        let memory_requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
        let memory_type_candidates =
            native_vulkan_vulkanalia_memory_type_candidates(memory_properties);
        let memory_type = sampled_image_memory_type_index(
            &memory_type_candidates,
            memory_requirements.memory_type_bits,
            HOST_VISIBLE_COHERENT_MEMORY_FLAG_BITS,
        )
        .or_else(|| {
            sampled_image_memory_type_index(
                &memory_type_candidates,
                memory_requirements.memory_type_bits,
                HOST_VISIBLE_MEMORY_FLAG_BITS,
            )
        })
        .ok_or_else(|| {
            format!(
                "scene-lite sampled image staging buffer has no host-visible memory type for bits 0x{:08x}",
                memory_requirements.memory_type_bits
            )
        })?;
        let allocation_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(memory_requirements.size)
            .memory_type_index(memory_type.index);
        let memory = unsafe { device.allocate_memory(&allocation_info, None) }.map_err(|err| {
            format!("vkAllocateMemory(vulkanalia scene-lite sampled image staging): {err:?}")
        })?;

        if let Err(err) = unsafe { device.bind_buffer_memory(buffer, memory, 0) } {
            unsafe {
                device.free_memory(memory, None);
            }
            return Err(format!(
                "vkBindBufferMemory(vulkanalia scene-lite sampled image staging): {err:?}"
            ));
        }

        let map = match unsafe {
            device.map_memory(memory, 0, payload.len() as u64, vk::MemoryMapFlags::empty())
        } {
            Ok(map) => map,
            Err(err) => {
                unsafe {
                    device.free_memory(memory, None);
                }
                return Err(format!(
                    "vkMapMemory(vulkanalia scene-lite sampled image staging): {err:?}"
                ));
            }
        };
        unsafe {
            ptr::copy_nonoverlapping(payload.as_ptr(), map.cast::<u8>(), payload.len());
        }
        let host_coherent = memory_type.property_flags_bits
            & vk::MemoryPropertyFlags::HOST_COHERENT.bits()
            == vk::MemoryPropertyFlags::HOST_COHERENT.bits();
        if !host_coherent {
            let range = vk::MappedMemoryRange::builder()
                .memory(memory)
                .offset(0)
                .size(vk::WHOLE_SIZE)
                .build();
            if let Err(err) = unsafe { device.flush_mapped_memory_ranges(&[range]) } {
                unsafe {
                    device.unmap_memory(memory);
                    device.free_memory(memory, None);
                }
                return Err(format!(
                    "vkFlushMappedMemoryRanges(vulkanalia scene-lite sampled image staging): {err:?}"
                ));
            }
        }
        unsafe {
            device.unmap_memory(memory);
        }

        Ok(VulkanaliaSceneLiteSampledImageUploadBuffer {
            buffer,
            memory,
            memory_type,
            size: payload.len() as u64,
        })
    })();

    if result.is_err() {
        unsafe {
            device.destroy_buffer(buffer, None);
        }
    }
    result
}

fn destroy_scene_lite_sampled_image_upload_buffer(
    device: &Device,
    buffer: VulkanaliaSceneLiteSampledImageUploadBuffer,
) {
    unsafe {
        device.destroy_buffer(buffer.buffer, None);
        device.free_memory(buffer.memory, None);
    }
}

fn scene_lite_sampled_image_view_create_info(image: vk::Image) -> vk::ImageViewCreateInfo {
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
        .format(vk::Format::R8G8B8A8_UNORM)
        .subresource_range(subresource_range)
        .build()
}

fn create_scene_lite_sampled_image_view(
    device: &Device,
    create_info: &vk::ImageViewCreateInfo,
) -> Result<vk::ImageView, String> {
    unsafe { device.create_image_view(create_info, None) }
        .map_err(|err| format!("vkCreateImageView(vulkanalia scene-lite sampled image): {err:?}"))
}

fn scene_lite_sampled_image_sampler_create_info(
    sampler_mode: NativeVulkanVulkanaliaSceneLiteSampledImageSamplerMode,
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

fn create_scene_lite_sampled_image_sampler(
    device: &Device,
    sampler_info: &vk::SamplerCreateInfo,
) -> Result<vk::Sampler, String> {
    unsafe { device.create_sampler(sampler_info, None) }
        .map_err(|err| format!("vkCreateSampler(vulkanalia scene-lite sampled image): {err:?}"))
}

fn upload_scene_lite_sampled_image(
    device: &Device,
    command_pool: vk::CommandPool,
    queue: vk::Queue,
    staging_buffer: vk::Buffer,
    image: vk::Image,
    extent: vk::Extent2D,
) -> Result<(), String> {
    let allocate_info = vk::CommandBufferAllocateInfo::builder()
        .command_pool(command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1);
    let command_buffers =
        unsafe { device.allocate_command_buffers(&allocate_info) }.map_err(|err| {
            format!("vkAllocateCommandBuffers(vulkanalia scene-lite sampled image upload): {err:?}")
        })?;
    let command_buffer = command_buffers[0];
    let result = record_and_submit_scene_lite_sampled_image_upload(
        device,
        queue,
        command_buffer,
        staging_buffer,
        image,
        extent,
    );
    unsafe {
        device.free_command_buffers(command_pool, &[command_buffer]);
    }
    result
}

fn record_and_submit_scene_lite_sampled_image_upload(
    device: &Device,
    queue: vk::Queue,
    command_buffer: vk::CommandBuffer,
    staging_buffer: vk::Buffer,
    image: vk::Image,
    extent: vk::Extent2D,
) -> Result<(), String> {
    unsafe {
        let begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
            .build();
        device
            .begin_command_buffer(command_buffer, &begin_info)
            .map_err(|err| {
                format!("vkBeginCommandBuffer(vulkanalia scene-lite sampled image upload): {err:?}")
            })?;

        let transfer_dst = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE)
            .src_access_mask(vk::AccessFlags2::empty())
            .dst_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(scene_lite_sampled_image_subresource_range())
            .build();
        let transfer_dst_barriers = [transfer_dst];
        let transfer_dst_dependency = vk::DependencyInfo::builder()
            .image_memory_barriers(&transfer_dst_barriers)
            .build();
        device.cmd_pipeline_barrier2(command_buffer, &transfer_dst_dependency);

        let image_subresource = vk::ImageSubresourceLayers::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .mip_level(0)
            .base_array_layer(0)
            .layer_count(1)
            .build();
        let image_copy = vk::BufferImageCopy::builder()
            .buffer_offset(0)
            .buffer_row_length(0)
            .buffer_image_height(0)
            .image_subresource(image_subresource)
            .image_extent(vk::Extent3D {
                width: extent.width,
                height: extent.height,
                depth: 1,
            })
            .build();
        device.cmd_copy_buffer_to_image(
            command_buffer,
            staging_buffer,
            image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[image_copy],
        );

        let shader_read = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
            .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(image)
            .subresource_range(scene_lite_sampled_image_subresource_range())
            .build();
        let shader_read_barriers = [shader_read];
        let shader_read_dependency = vk::DependencyInfo::builder()
            .image_memory_barriers(&shader_read_barriers)
            .build();
        device.cmd_pipeline_barrier2(command_buffer, &shader_read_dependency);

        device.end_command_buffer(command_buffer).map_err(|err| {
            format!("vkEndCommandBuffer(vulkanalia scene-lite sampled image upload): {err:?}")
        })?;
    }

    let fence_info = vk::FenceCreateInfo::builder();
    let fence = unsafe { device.create_fence(&fence_info, None) }.map_err(|err| {
        format!("vkCreateFence(vulkanalia scene-lite sampled image upload): {err:?}")
    })?;
    let result = submit_scene_lite_sampled_image_upload_command_buffer2(
        device,
        queue,
        command_buffer,
        fence,
    )
    .and_then(|()| unsafe {
        device
            .wait_for_fences(&[fence], true, u64::MAX)
            .map(|_| ())
            .map_err(|err| {
                format!("vkWaitForFences(vulkanalia scene-lite sampled image upload): {err:?}")
            })
    });
    unsafe {
        device.destroy_fence(fence, None);
    }
    result
}

fn submit_scene_lite_sampled_image_upload_command_buffer2(
    device: &Device,
    queue: vk::Queue,
    command_buffer: vk::CommandBuffer,
    fence: vk::Fence,
) -> Result<(), String> {
    let command_buffer_info = vk::CommandBufferSubmitInfo::builder()
        .command_buffer(command_buffer)
        .build();
    let command_buffer_infos = [command_buffer_info];
    let submit_info = vk::SubmitInfo2::builder()
        .command_buffer_infos(&command_buffer_infos)
        .build();
    unsafe {
        device
            .queue_submit2(queue, &[submit_info], fence)
            .map_err(|err| {
                format!("vkQueueSubmit2(vulkanalia scene-lite sampled image upload): {err:?}")
            })
    }
}

fn scene_lite_sampled_image_subresource_range() -> vk::ImageSubresourceRange {
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
        "decode_source_image_rgba",
        "create_sampled_image_transfer_dst_sampled",
        "create_rgba_upload_staging_buffer",
        "cmd_pipeline_barrier2_transfer_dst",
        "cmd_copy_buffer_to_image",
        "cmd_pipeline_barrier2_shader_read",
        "create_combined_image_sampler_descriptor",
        "queue_submit2_upload",
        "wait_upload_fence",
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
        let snapshot = native_vulkan_vulkanalia_scene_lite_sampled_image_plan(
            NativeVulkanVulkanaliaSceneLiteSampledImagePlanInput {
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
        assert_eq!(snapshot.sampled_image_format, "R8G8B8A8_UNORM");
        assert_eq!(
            snapshot.sampled_image_usage,
            vec!["transfer-dst", "sampled"]
        );
        assert_eq!(
            snapshot.image_layout_flow,
            vec![
                "undefined",
                "transfer-dst-optimal",
                "shader-read-only-optimal"
            ]
        );
        assert!(snapshot.command_order.contains(&"cmd_copy_buffer_to_image"));
        assert!(
            snapshot
                .command_order
                .contains(&"cmd_bind_scene_lite_descriptor_heap")
        );
        assert_eq!(snapshot.draw_indexed_count, 1);
        assert!(snapshot.uses_dynamic_rendering);
        assert!(snapshot.uses_synchronization2);
        assert!(snapshot.uses_submit2);
        assert!(!snapshot.uses_push_descriptor_fast_path);
    }

    #[test]
    fn sampled_image_plan_rejects_incomplete_geometry() {
        let snapshot = native_vulkan_vulkanalia_scene_lite_sampled_image_plan(
            NativeVulkanVulkanaliaSceneLiteSampledImagePlanInput {
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
            vec!["wait_for_scene_lite_sampled_image_geometry"]
        );
    }

    #[test]
    fn descriptor_strategy_requires_descriptor_heap_even_when_push_descriptor_exists() {
        let snapshot = native_vulkan_vulkanalia_scene_lite_sampled_image_descriptor_strategy(
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
        let snapshot = native_vulkan_vulkanalia_scene_lite_sampled_image_descriptor_strategy(
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
        let snapshot = native_vulkan_vulkanalia_scene_lite_sampled_image_descriptor_strategy(
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
    fn rgba_upload_validation_matches_extent() {
        let extent = vk::Extent2D {
            width: 2,
            height: 2,
        };

        assert_eq!(scene_lite_rgba_byte_len(extent), Ok(16));
        assert!(validate_scene_lite_rgba_upload(extent, &[0; 16]).is_ok());
        assert!(validate_scene_lite_rgba_upload(extent, &[0; 15]).is_err());
        assert!(
            validate_scene_lite_rgba_upload(
                vk::Extent2D {
                    width: 0,
                    height: 2
                },
                &[]
            )
            .is_err()
        );
    }

    #[test]
    fn sampled_image_sampler_modes_name_vulkan_address_modes() {
        assert_eq!(
            NativeVulkanVulkanaliaSceneLiteSampledImageSamplerMode::ClampToEdge.address_mode(),
            vk::SamplerAddressMode::CLAMP_TO_EDGE
        );
        assert_eq!(
            NativeVulkanVulkanaliaSceneLiteSampledImageSamplerMode::ClampToEdge.label(),
            "clamp-to-edge"
        );
        assert_eq!(
            NativeVulkanVulkanaliaSceneLiteSampledImageSamplerMode::Repeat.address_mode(),
            vk::SamplerAddressMode::REPEAT
        );
        assert_eq!(
            NativeVulkanVulkanaliaSceneLiteSampledImageSamplerMode::Repeat.label(),
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
                "decode_source_image_rgba",
                "create_sampled_image_transfer_dst_sampled",
                "create_rgba_upload_staging_buffer",
                "cmd_pipeline_barrier2_transfer_dst",
                "cmd_copy_buffer_to_image",
                "cmd_pipeline_barrier2_shader_read",
                "create_combined_image_sampler_descriptor",
                "queue_submit2_upload",
                "wait_upload_fence",
                "retain_sampled_image_descriptor_for_present",
            ]
        );
    }

    #[test]
    fn decodes_scene_lite_image_to_rgba_bytes() {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "gilder-scene-lite-rgba-decode-{}.png",
            std::process::id()
        ));
        let image = image::RgbaImage::from_raw(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 128])
            .expect("test rgba image");
        image.save(&path).expect("write test png");

        let decoded =
            native_vulkan_vulkanalia_decode_scene_lite_rgba_image(&path).expect("decode png");
        let _ = std::fs::remove_file(&path);

        assert_eq!(decoded.width, 2);
        assert_eq!(decoded.height, 1);
        assert_eq!(decoded.bytes, vec![255, 0, 0, 255, 0, 255, 0, 128]);
    }
}
