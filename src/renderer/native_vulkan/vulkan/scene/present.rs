#![allow(dead_code)]

use std::path::PathBuf;
use std::ptr;
use std::thread;
use std::time::{Duration, Instant};

use serde::Serialize;
use vulkanalia::Version;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{
    self, HasBuilder, KhrSurfaceExtensionInstanceCommands, KhrSwapchainExtensionDeviceCommands,
};

use crate::core::{FitMode, SceneSize, SceneTextureRegion};
use crate::renderer::native_vulkan::NativeVulkanClearColor;
use crate::renderer::native_wayland::{
    NativeWaylandHost, NativeWaylandHostOptions, NativeWaylandSurfaceHandles,
};

use super::descriptor_heap::{
    NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanInput,
    NativeVulkanVulkanaliaDescriptorHeapImageSamplerResourceSnapshot,
    VulkanaliaDescriptorHeapImageSamplerResources,
    native_vulkan_vulkanalia_create_descriptor_heap_image_sampler_resources,
    native_vulkan_vulkanalia_descriptor_heap_image_sampler_plan,
    native_vulkan_vulkanalia_destroy_descriptor_heap_image_sampler_resources,
    native_vulkan_vulkanalia_write_descriptor_heap_image_sampler,
};
use super::instance::{
    NativeVulkanVulkanaliaInstance,
    native_vulkan_vulkanalia_create_instance_with_required_extensions,
    native_vulkan_vulkanalia_destroy_instance,
};
use super::memory::{
    native_vulkan_vulkanalia_bind_buffer_memory2, native_vulkan_vulkanalia_map_memory2,
    native_vulkan_vulkanalia_unmap_memory2,
};
use super::present_timing::VulkanaliaPresentTimingConfig;
use super::scene_draw_pass::{
    NativeVulkanVulkanaliaSceneSampledImageCommandSnapshot,
    NativeVulkanVulkanaliaSceneSampledImagePipelineSnapshot,
    NativeVulkanVulkanaliaSceneSolidQuadCommandSnapshot,
    NativeVulkanVulkanaliaSceneSolidQuadPipelineSnapshot,
    VulkanaliaSceneDescriptorHeapDrawResources, VulkanaliaSceneSampledImageDescriptorBinding,
    VulkanaliaSceneSampledImageDrawCommand, VulkanaliaSceneSampledImagePipelineResources,
    VulkanaliaSceneSolidQuadDrawCommand, VulkanaliaSceneSolidQuadDrawResources,
    VulkanaliaSceneSolidQuadPipelineResources,
    native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_resources,
    native_vulkan_vulkanalia_create_scene_solid_quad_pipeline_resources,
    native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_resources,
    native_vulkan_vulkanalia_destroy_scene_solid_quad_pipeline_resources,
    native_vulkan_vulkanalia_record_scene_sampled_image_command_buffer,
    native_vulkan_vulkanalia_record_scene_solid_quad_command_buffer,
};
use super::scene_sampled_image::{
    NativeVulkanVulkanaliaSceneNativeTexture,
    NativeVulkanVulkanaliaSceneSampledImageDescriptorStrategySnapshot,
    NativeVulkanVulkanaliaSceneSampledImageResourceSnapshot,
    NativeVulkanVulkanaliaSceneSampledImageSamplerMode, VulkanaliaSceneSampledImageResources,
    native_vulkan_vulkanalia_create_scene_sampled_image_resources,
    native_vulkan_vulkanalia_destroy_scene_sampled_image_resources,
    native_vulkan_vulkanalia_load_scene_native_texture,
    native_vulkan_vulkanalia_scene_sampled_image_descriptor_strategy,
    native_vulkan_vulkanalia_trim_scene_sampled_image_decode_heap,
};
use super::swapchain::{
    NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot,
    NativeVulkanVulkanaliaPresentQueueSnapshot, NativeVulkanVulkanaliaSwapchainSnapshot,
    OPTIONAL_INSTANCE_EXTENSIONS, REQUIRED_INSTANCE_EXTENSIONS, composite_alpha_label,
    create_vulkanalia_present_device, create_vulkanalia_swapchain_plan,
    create_vulkanalia_wayland_surface, present_mode_label, queue_flag_labels,
    select_vulkanalia_present_queue, swapchain_create_flag_labels,
    vulkanalia_surface_capabilities2_enabled, vulkanalia_surface_maintenance1_enabled,
};
use super::video_session::{
    NativeVulkanVulkanaliaMemoryTypeCandidate, native_vulkan_vulkanalia_memory_type_candidates,
};

const SCENE_FULL_SOLID_QUAD_INDEX_COUNT: u32 = 6;
const SCENE_FULL_SOLID_QUAD_VERTEX_STRIDE_BYTES: u32 = 24;
const SCENE_FULL_SAMPLED_IMAGE_INDEX_COUNT: u32 = 6;
const SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES: u32 = 20;
const HOST_VISIBLE_COHERENT_MEMORY_FLAG_BITS: u32 =
    vk::MemoryPropertyFlags::HOST_VISIBLE.bits() | vk::MemoryPropertyFlags::HOST_COHERENT.bits();
const HOST_VISIBLE_COHERENT_DEVICE_LOCAL_MEMORY_FLAG_BITS: u32 =
    HOST_VISIBLE_COHERENT_MEMORY_FLAG_BITS | vk::MemoryPropertyFlags::DEVICE_LOCAL.bits();
const HOST_VISIBLE_MEMORY_FLAG_BITS: u32 = vk::MemoryPropertyFlags::HOST_VISIBLE.bits();

pub type NativeVulkanVulkanaliaSceneSolidQuadDynamicGeometry = Box<
    dyn Fn(u64) -> Result<NativeVulkanVulkanaliaSceneSolidQuadGeometryInput, String> + Send + Sync,
>;
pub type NativeVulkanVulkanaliaSceneSampledImageDynamicGeometry = Box<
    dyn Fn(u64) -> Result<NativeVulkanVulkanaliaSceneSampledImageGeometryInput, String>
        + Send
        + Sync,
>;
pub type NativeVulkanVulkanaliaSceneMixedSolidQuadDynamicGeometry = Box<
    dyn Fn(u64) -> Result<Option<NativeVulkanVulkanaliaSceneSolidQuadGeometryInput>, String>
        + Send
        + Sync,
>;

pub struct NativeVulkanVulkanaliaSceneSolidQuadPresentOptions {
    pub host: NativeWaylandHostOptions,
    pub wait_configure_roundtrips: usize,
    pub duration: Duration,
    pub target_max_fps: Option<u32>,
    pub quad_color: NativeVulkanClearColor,
    pub geometry: Option<NativeVulkanVulkanaliaSceneSolidQuadGeometryInput>,
    pub dynamic_geometry: Option<NativeVulkanVulkanaliaSceneSolidQuadDynamicGeometry>,
    pub scene_size: Option<SceneSize>,
    pub scene_fit: FitMode,
}

pub struct NativeVulkanVulkanaliaSceneSampledImagePresentOptions {
    pub host: NativeWaylandHostOptions,
    pub wait_configure_roundtrips: usize,
    pub duration: Duration,
    pub target_max_fps: Option<u32>,
    pub source: PathBuf,
    pub clear_color: NativeVulkanClearColor,
    pub fit: Option<FitMode>,
    pub solid_geometry: Option<NativeVulkanVulkanaliaSceneSolidQuadGeometryInput>,
    pub geometry: Option<NativeVulkanVulkanaliaSceneSampledImageGeometryInput>,
    pub dynamic_solid_geometry: Option<NativeVulkanVulkanaliaSceneMixedSolidQuadDynamicGeometry>,
    pub dynamic_geometry: Option<NativeVulkanVulkanaliaSceneSampledImageDynamicGeometry>,
    pub scene_size: Option<SceneSize>,
    pub scene_fit: FitMode,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeVulkanVulkanaliaSceneSolidQuadVertex {
    pub position: [f32; 2],
    pub rgba: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeVulkanVulkanaliaSceneSampledImageVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub opacity: f32,
}

impl NativeVulkanVulkanaliaSceneSampledImageVertex {
    pub fn new(position: [f32; 2], uv: [f32; 2], opacity: f32) -> Self {
        Self {
            position,
            uv,
            opacity,
        }
    }
}

impl NativeVulkanVulkanaliaSceneSolidQuadVertex {
    pub fn new(position: [f32; 2], rgba: [f32; 4]) -> Self {
        Self { position, rgba }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NativeVulkanVulkanaliaSceneSolidQuadGeometryInput {
    pub vertices: Vec<NativeVulkanVulkanaliaSceneSolidQuadVertex>,
    pub indices: Vec<u32>,
    pub draw_steps: Vec<NativeVulkanVulkanaliaSceneSolidQuadDrawStep>,
    pub source_label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NativeVulkanVulkanaliaSceneSampledImageGeometryInput {
    pub vertices: Vec<NativeVulkanVulkanaliaSceneSampledImageVertex>,
    pub indices: Vec<u32>,
    pub sources: Vec<PathBuf>,
    pub draw_steps: Vec<NativeVulkanVulkanaliaSceneSampledImageDrawStep>,
    pub source_label: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeVulkanVulkanaliaSceneSampledImageDrawStep {
    pub layer_index: usize,
    pub resource_index: u32,
    pub first_index: u32,
    pub index_count: u32,
    pub fit: Option<FitMode>,
    pub texture_region: Option<SceneTextureRegion>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeVulkanVulkanaliaSceneSolidQuadDrawStep {
    pub layer_index: usize,
    pub first_index: u32,
    pub index_count: u32,
}

impl NativeVulkanVulkanaliaSceneSampledImageGeometryInput {
    pub fn new(
        vertices: Vec<NativeVulkanVulkanaliaSceneSampledImageVertex>,
        indices: Vec<u32>,
        source_label: impl Into<String>,
    ) -> Self {
        let index_count = indices.len().min(u32::MAX as usize) as u32;
        Self {
            vertices,
            indices,
            sources: Vec::new(),
            draw_steps: vec![NativeVulkanVulkanaliaSceneSampledImageDrawStep {
                layer_index: 0,
                resource_index: 0,
                first_index: 0,
                index_count,
                fit: None,
                texture_region: None,
            }],
            source_label: source_label.into(),
        }
    }

    pub fn new_batched(
        vertices: Vec<NativeVulkanVulkanaliaSceneSampledImageVertex>,
        indices: Vec<u32>,
        sources: Vec<PathBuf>,
        draw_steps: Vec<NativeVulkanVulkanaliaSceneSampledImageDrawStep>,
        source_label: impl Into<String>,
    ) -> Self {
        Self {
            vertices,
            indices,
            sources,
            draw_steps,
            source_label: source_label.into(),
        }
    }
}

impl NativeVulkanVulkanaliaSceneSolidQuadGeometryInput {
    pub fn new(
        vertices: Vec<NativeVulkanVulkanaliaSceneSolidQuadVertex>,
        indices: Vec<u32>,
        source_label: impl Into<String>,
    ) -> Self {
        let index_count = indices.len().min(u32::MAX as usize) as u32;
        Self::new_batched(
            vertices,
            indices,
            vec![NativeVulkanVulkanaliaSceneSolidQuadDrawStep {
                layer_index: 0,
                first_index: 0,
                index_count,
            }],
            source_label,
        )
    }

    pub fn new_batched(
        vertices: Vec<NativeVulkanVulkanaliaSceneSolidQuadVertex>,
        indices: Vec<u32>,
        draw_steps: Vec<NativeVulkanVulkanaliaSceneSolidQuadDrawStep>,
        source_label: impl Into<String>,
    ) -> Self {
        Self {
            vertices,
            indices,
            draw_steps,
            source_label: source_label.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanVulkanaliaSceneSolidQuadPresentSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub loader: String,
    pub requested_api_version: String,
    pub runtime_elapsed_ms: u64,
    pub frames_presented: u64,
    pub average_present_fps: f64,
    pub quad_color: NativeVulkanClearColor,
    pub selected_queue: NativeVulkanVulkanaliaPresentQueueSnapshot,
    pub device_extensions: NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot,
    pub swapchain: NativeVulkanVulkanaliaSwapchainSnapshot,
    pub geometry: NativeVulkanVulkanaliaSceneSolidQuadGeometrySnapshot,
    pub pipeline: NativeVulkanVulkanaliaSceneSolidQuadPipelineSnapshot,
    pub last_command: Option<NativeVulkanVulkanaliaSceneSolidQuadCommandSnapshot>,
    pub command_submit_model: &'static str,
    pub present_sync_model: &'static str,
    pub wait_idle_after_present: bool,
    pub present_ids: Vec<Option<u64>>,
    pub uses_present_id2: bool,
    pub present_wait2_available: bool,
    pub present_wait_after_present: bool,
    pub uses_pipeline_rendering_create_info: bool,
    pub uses_dynamic_rendering: bool,
    pub uses_synchronization2: bool,
    pub uses_submit2: bool,
    pub zero_copy_scope: &'static str,
    pub primary_reference: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanVulkanaliaSceneSampledImagePresentSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub scene_input_model: &'static str,
    pub scene_resource_model: &'static str,
    pub scene_solid_quad_draw_count: u32,
    pub scene_sampled_image_resource_count: u32,
    pub scene_sampled_image_descriptor_heap_required: bool,
    pub loader: String,
    pub requested_api_version: String,
    pub runtime_elapsed_ms: u64,
    pub frames_presented: u64,
    pub average_present_fps: f64,
    pub source: PathBuf,
    pub clear_color: NativeVulkanClearColor,
    pub fit: Option<FitMode>,
    pub mixed_scene_draw_enabled: bool,
    pub selected_queue: NativeVulkanVulkanaliaPresentQueueSnapshot,
    pub device_extensions: NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot,
    pub swapchain: NativeVulkanVulkanaliaSwapchainSnapshot,
    pub solid_geometry: Option<NativeVulkanVulkanaliaSceneSolidQuadGeometrySnapshot>,
    pub solid_pipeline: Option<NativeVulkanVulkanaliaSceneSolidQuadPipelineSnapshot>,
    pub geometry: NativeVulkanVulkanaliaSceneSampledImageGeometrySnapshot,
    pub sampled_image: NativeVulkanVulkanaliaSceneSampledImageResourceSnapshot,
    pub sampled_images: Vec<NativeVulkanVulkanaliaSceneSampledImageResourceSnapshot>,
    pub descriptor_strategy: NativeVulkanVulkanaliaSceneSampledImageDescriptorStrategySnapshot,
    pub descriptor_heap: Option<NativeVulkanVulkanaliaDescriptorHeapImageSamplerResourceSnapshot>,
    pub pipeline: NativeVulkanVulkanaliaSceneSampledImagePipelineSnapshot,
    pub last_command: Option<NativeVulkanVulkanaliaSceneSampledImageCommandSnapshot>,
    pub command_submit_model: &'static str,
    pub present_sync_model: &'static str,
    pub wait_idle_after_present: bool,
    pub present_ids: Vec<Option<u64>>,
    pub uses_present_id2: bool,
    pub present_wait2_available: bool,
    pub present_wait_after_present: bool,
    pub uses_pipeline_rendering_create_info: bool,
    pub uses_dynamic_rendering: bool,
    pub uses_synchronization2: bool,
    pub uses_submit2: bool,
    pub zero_copy_scope: &'static str,
    pub primary_reference: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSceneSolidQuadGeometrySnapshot {
    pub source_label: String,
    pub vertex_count: u32,
    pub vertex_buffer_bytes: u64,
    pub index_buffer_bytes: u64,
    pub index_count: u32,
    pub quad_count: u32,
    pub draw_step_count: u32,
    pub vertex_stride_bytes: u32,
    pub selected_vertex_memory_type_index: u32,
    pub selected_index_memory_type_index: u32,
    pub vertex_memory_property_flags: Vec<&'static str>,
    pub index_memory_property_flags: Vec<&'static str>,
    pub upload_model: &'static str,
    pub retained_across_frames: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSceneSampledImageGeometrySnapshot {
    pub source_label: String,
    pub vertex_count: u32,
    pub vertex_buffer_bytes: u64,
    pub vertex_buffer_count: u32,
    pub index_buffer_bytes: u64,
    pub index_count: u32,
    pub quad_count: u32,
    pub source_count: u32,
    pub draw_step_count: u32,
    pub vertex_stride_bytes: u32,
    pub selected_vertex_memory_type_index: u32,
    pub selected_index_memory_type_index: u32,
    pub vertex_memory_property_flags: Vec<&'static str>,
    pub index_memory_property_flags: Vec<&'static str>,
    pub upload_model: &'static str,
    pub retained_across_frames: bool,
}

struct VulkanaliaSceneSolidQuadGeometryResources {
    vertex_buffers: Vec<VulkanaliaSceneUploadedBuffer>,
    index_buffer: vk::Buffer,
    index_memory: vk::DeviceMemory,
    draw_steps: Vec<NativeVulkanVulkanaliaSceneSolidQuadDrawStep>,
    indices: Vec<u32>,
    snapshot: NativeVulkanVulkanaliaSceneSolidQuadGeometrySnapshot,
}

struct VulkanaliaSceneSampledImageGeometryResources {
    vertex_buffers: Vec<VulkanaliaSceneUploadedBuffer>,
    index_buffer: vk::Buffer,
    index_memory: vk::DeviceMemory,
    draw_steps: Vec<NativeVulkanVulkanaliaSceneSampledImageDrawStep>,
    base_vertices: Vec<NativeVulkanVulkanaliaSceneSampledImageVertex>,
    indices: Vec<u32>,
    sources: Vec<PathBuf>,
    snapshot: NativeVulkanVulkanaliaSceneSampledImageGeometrySnapshot,
}

struct VulkanaliaSceneSolidQuadFrameResources {
    swapchain_image_views: Vec<vk::ImageView>,
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    image_available: Vec<vk::Semaphore>,
    render_finished: Vec<vk::Semaphore>,
    in_flight: Vec<vk::Fence>,
}

struct VulkanaliaSceneUploadedBuffer {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    memory_type: NativeVulkanVulkanaliaMemoryTypeCandidate,
}

#[derive(Debug)]
struct VulkanaliaSceneSolidQuadGeometryPayload {
    indices: Vec<u32>,
    vertex_bytes: Vec<u8>,
    index_bytes: Vec<u8>,
    vertex_count: u32,
    index_count: u32,
    quad_count: u32,
    draw_steps: Vec<NativeVulkanVulkanaliaSceneSolidQuadDrawStep>,
    source_label: String,
}

#[derive(Debug)]
struct VulkanaliaSceneSampledImageGeometryPayload {
    vertices: Vec<NativeVulkanVulkanaliaSceneSampledImageVertex>,
    indices: Vec<u32>,
    sources: Vec<PathBuf>,
    vertex_bytes: Vec<u8>,
    index_bytes: Vec<u8>,
    vertex_count: u32,
    index_count: u32,
    quad_count: u32,
    source_count: u32,
    draw_steps: Vec<NativeVulkanVulkanaliaSceneSampledImageDrawStep>,
    source_label: String,
}

pub fn run_native_vulkan_vulkanalia_scene_solid_quad_present(
    options: NativeVulkanVulkanaliaSceneSolidQuadPresentOptions,
) -> Result<NativeVulkanVulkanaliaSceneSolidQuadPresentSnapshot, String> {
    let mut host =
        NativeWaylandHost::connect(options.host.clone()).map_err(|err| err.to_string())?;
    host.wait_until_configured(options.wait_configure_roundtrips)
        .map_err(|err| err.to_string())?;
    let handles = host.surface_handles().map_err(|err| err.to_string())?;

    let mut requested_instance_extensions = REQUIRED_INSTANCE_EXTENSIONS.to_vec();
    requested_instance_extensions.extend_from_slice(OPTIONAL_INSTANCE_EXTENSIONS);
    let vulkan = native_vulkan_vulkanalia_create_instance_with_required_extensions(
        &requested_instance_extensions,
    )?;
    let result = run_vulkanalia_scene_solid_quad_present_inner(&vulkan, handles, options);
    native_vulkan_vulkanalia_destroy_instance(vulkan);
    result
}

fn run_vulkanalia_scene_solid_quad_present_inner(
    vulkan: &NativeVulkanVulkanaliaInstance,
    handles: NativeWaylandSurfaceHandles,
    options: NativeVulkanVulkanaliaSceneSolidQuadPresentOptions,
) -> Result<NativeVulkanVulkanaliaSceneSolidQuadPresentSnapshot, String> {
    let instance = &vulkan.instance;
    let surface = create_vulkanalia_wayland_surface(instance, handles)?;
    let result =
        with_vulkanalia_scene_solid_quad_present(instance, surface, handles, vulkan, options);
    unsafe {
        instance.destroy_surface_khr(surface, None);
    }
    result
}

pub fn run_native_vulkan_vulkanalia_scene_sampled_image_present(
    options: NativeVulkanVulkanaliaSceneSampledImagePresentOptions,
) -> Result<NativeVulkanVulkanaliaSceneSampledImagePresentSnapshot, String> {
    let mut host =
        NativeWaylandHost::connect(options.host.clone()).map_err(|err| err.to_string())?;
    host.wait_until_configured(options.wait_configure_roundtrips)
        .map_err(|err| err.to_string())?;
    let handles = host.surface_handles().map_err(|err| err.to_string())?;

    let mut requested_instance_extensions = REQUIRED_INSTANCE_EXTENSIONS.to_vec();
    requested_instance_extensions.extend_from_slice(OPTIONAL_INSTANCE_EXTENSIONS);
    let vulkan = native_vulkan_vulkanalia_create_instance_with_required_extensions(
        &requested_instance_extensions,
    )?;
    let result = run_vulkanalia_scene_sampled_image_present_inner(&vulkan, handles, options);
    native_vulkan_vulkanalia_destroy_instance(vulkan);
    result
}

fn run_vulkanalia_scene_sampled_image_present_inner(
    vulkan: &NativeVulkanVulkanaliaInstance,
    handles: NativeWaylandSurfaceHandles,
    options: NativeVulkanVulkanaliaSceneSampledImagePresentOptions,
) -> Result<NativeVulkanVulkanaliaSceneSampledImagePresentSnapshot, String> {
    let instance = &vulkan.instance;
    let surface = create_vulkanalia_wayland_surface(instance, handles)?;
    let result =
        with_vulkanalia_scene_sampled_image_present(instance, surface, handles, vulkan, options);
    unsafe {
        instance.destroy_surface_khr(surface, None);
    }
    result
}

fn with_vulkanalia_scene_solid_quad_present(
    instance: &Instance,
    surface: vk::SurfaceKHR,
    handles: NativeWaylandSurfaceHandles,
    vulkan: &NativeVulkanVulkanaliaInstance,
    options: NativeVulkanVulkanaliaSceneSolidQuadPresentOptions,
) -> Result<NativeVulkanVulkanaliaSceneSolidQuadPresentSnapshot, String> {
    let physical_devices = unsafe { instance.enumerate_physical_devices() }
        .map_err(|err| format!("vkEnumeratePhysicalDevices(vulkanalia scene present): {err:?}"))?;
    let mut present_queue_family_count = 0usize;
    let selection = select_vulkanalia_present_queue(
        instance,
        surface,
        handles,
        &physical_devices,
        &mut present_queue_family_count,
    )?;
    let present_device = create_vulkanalia_present_device(
        instance,
        &selection,
        vulkanalia_surface_maintenance1_enabled(vulkan),
    )?;
    if !present_device.feature_selection.synchronization2_enabled {
        unsafe {
            present_device.device.destroy_device(None);
        }
        return Err(
            "Vulkanalia scene present requires synchronization2 for QueueSubmit2".to_owned(),
        );
    }
    if !present_device.feature_selection.dynamic_rendering_enabled {
        unsafe {
            present_device.device.destroy_device(None);
        }
        return Err(
            "Vulkanalia scene present requires dynamicRendering for CmdBeginRendering".to_owned(),
        );
    }

    let swapchain_plan = match create_vulkanalia_swapchain_plan(
        instance,
        selection.physical_device,
        surface,
        handles.buffer_size,
        vulkanalia_surface_capabilities2_enabled(vulkan),
        &present_device.feature_selection,
    ) {
        Ok(plan) => plan,
        Err(err) => {
            unsafe {
                present_device.device.destroy_device(None);
            }
            return Err(err);
        }
    };
    let device = &present_device.device;
    let swapchain = match unsafe { device.create_swapchain_khr(&swapchain_plan.create_info, None) }
    {
        Ok(swapchain) => swapchain,
        Err(err) => {
            unsafe {
                present_device.device.destroy_device(None);
            }
            return Err(format!(
                "vkCreateSwapchainKHR(vulkanalia scene present): {err:?}"
            ));
        }
    };
    let swapchain_images = match unsafe { device.get_swapchain_images_khr(swapchain) } {
        Ok(images) => images,
        Err(err) => {
            unsafe {
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(format!(
                "vkGetSwapchainImagesKHR(vulkanalia scene present): {err:?}"
            ));
        }
    };

    let frame_resources = match create_scene_solid_quad_frame_resources(
        device,
        &swapchain_images,
        swapchain_plan.format.format,
        selection.queue_family_index,
    ) {
        Ok(resources) => resources,
        Err(err) => {
            unsafe {
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(err);
        }
    };
    let pipeline = match native_vulkan_vulkanalia_create_scene_solid_quad_pipeline_resources(
        device,
        swapchain_plan.format.format,
        swapchain_plan.extent,
    ) {
        Ok(pipeline) => pipeline,
        Err(err) => {
            destroy_scene_solid_quad_frame_resources(device, frame_resources);
            unsafe {
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(err);
        }
    };
    let memory_properties =
        unsafe { instance.get_physical_device_memory_properties(selection.physical_device) };
    let geometry_payload = match scene_solid_quad_geometry_payload(
        options.geometry.as_ref(),
        swapchain_plan.extent,
        options.quad_color,
        options.scene_size,
        options.scene_fit,
    ) {
        Ok(payload) => payload,
        Err(err) => {
            native_vulkan_vulkanalia_destroy_scene_solid_quad_pipeline_resources(device, pipeline);
            destroy_scene_solid_quad_frame_resources(device, frame_resources);
            unsafe {
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(err);
        }
    };
    let geometry = match create_scene_solid_quad_geometry_resources(
        device,
        &memory_properties,
        geometry_payload,
        if options.dynamic_geometry.is_some() {
            frame_resources.in_flight.len()
        } else {
            1
        },
    ) {
        Ok(geometry) => geometry,
        Err(err) => {
            native_vulkan_vulkanalia_destroy_scene_solid_quad_pipeline_resources(device, pipeline);
            destroy_scene_solid_quad_frame_resources(device, frame_resources);
            unsafe {
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(err);
        }
    };
    let present_timing = VulkanaliaPresentTimingConfig::new(
        swapchain_plan.present_id2_enabled,
        swapchain_plan.present_wait2_enabled,
    );

    let result = run_scene_solid_quad_present_loop(
        vulkan,
        device,
        present_device.queue,
        swapchain,
        &swapchain_images,
        swapchain_plan.extent,
        &frame_resources,
        &pipeline,
        &geometry,
        &selection,
        &present_device.extension_snapshot,
        &swapchain_plan,
        present_timing,
        options,
    );

    let _ = unsafe { device.device_wait_idle() };
    destroy_scene_solid_quad_geometry_resources(device, geometry);
    native_vulkan_vulkanalia_destroy_scene_solid_quad_pipeline_resources(device, pipeline);
    destroy_scene_solid_quad_frame_resources(device, frame_resources);
    unsafe {
        device.destroy_swapchain_khr(swapchain, None);
        present_device.device.destroy_device(None);
    }

    result
}

fn with_vulkanalia_scene_sampled_image_present(
    instance: &Instance,
    surface: vk::SurfaceKHR,
    handles: NativeWaylandSurfaceHandles,
    vulkan: &NativeVulkanVulkanaliaInstance,
    options: NativeVulkanVulkanaliaSceneSampledImagePresentOptions,
) -> Result<NativeVulkanVulkanaliaSceneSampledImagePresentSnapshot, String> {
    let physical_devices = unsafe { instance.enumerate_physical_devices() }.map_err(|err| {
        format!("vkEnumeratePhysicalDevices(vulkanalia scene sampled image present): {err:?}")
    })?;
    let mut present_queue_family_count = 0usize;
    let selection = select_vulkanalia_present_queue(
        instance,
        surface,
        handles,
        &physical_devices,
        &mut present_queue_family_count,
    )?;
    let present_device = create_vulkanalia_present_device(
        instance,
        &selection,
        vulkanalia_surface_maintenance1_enabled(vulkan),
    )?;
    if !present_device.feature_selection.synchronization2_enabled {
        unsafe {
            present_device.device.destroy_device(None);
        }
        return Err(
            "Vulkanalia scene sampled image present requires synchronization2 for QueueSubmit2"
                .to_owned(),
        );
    }
    if !present_device.feature_selection.dynamic_rendering_enabled {
        unsafe {
            present_device.device.destroy_device(None);
        }
        return Err("Vulkanalia scene sampled image present requires dynamicRendering for CmdBeginRendering".to_owned());
    }

    let swapchain_plan = match create_vulkanalia_swapchain_plan(
        instance,
        selection.physical_device,
        surface,
        handles.buffer_size,
        vulkanalia_surface_capabilities2_enabled(vulkan),
        &present_device.feature_selection,
    ) {
        Ok(plan) => plan,
        Err(err) => {
            unsafe {
                present_device.device.destroy_device(None);
            }
            return Err(err);
        }
    };
    let device = &present_device.device;
    let swapchain = match unsafe { device.create_swapchain_khr(&swapchain_plan.create_info, None) }
    {
        Ok(swapchain) => swapchain,
        Err(err) => {
            unsafe {
                present_device.device.destroy_device(None);
            }
            return Err(format!(
                "vkCreateSwapchainKHR(vulkanalia scene sampled image present): {err:?}"
            ));
        }
    };
    let swapchain_images = match unsafe { device.get_swapchain_images_khr(swapchain) } {
        Ok(images) => images,
        Err(err) => {
            unsafe {
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(format!(
                "vkGetSwapchainImagesKHR(vulkanalia scene sampled image present): {err:?}"
            ));
        }
    };

    let frame_resources = match create_scene_solid_quad_frame_resources(
        device,
        &swapchain_images,
        swapchain_plan.format.format,
        selection.queue_family_index,
    ) {
        Ok(resources) => resources,
        Err(err) => {
            unsafe {
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(err);
        }
    };
    let sampled_image_sources =
        scene_sampled_image_sources(&options.source, options.geometry.as_ref());
    if !present_device
        .feature_selection
        .core_features
        .texture_compression_bc
    {
        destroy_scene_solid_quad_frame_resources(device, frame_resources);
        unsafe {
            device.destroy_swapchain_khr(swapchain, None);
            present_device.device.destroy_device(None);
        }
        return Err(
            "scene sampled-image runtime requires textureCompressionBC for native BC7 .gtex resources"
                .to_owned(),
        );
    }
    let descriptor_strategy = native_vulkan_vulkanalia_scene_sampled_image_descriptor_strategy(
        present_device.feature_selection.core_features,
        present_device.feature_selection.vulkan_1_4_properties,
        present_device.feature_selection.descriptor_heap_properties,
        sampled_image_sources.len(),
    );
    let descriptor_heap_plan = native_vulkan_vulkanalia_descriptor_heap_image_sampler_plan(
        NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanInput {
            image_count: sampled_image_sources.len(),
            properties: present_device.feature_selection.descriptor_heap_properties,
        },
    );
    let use_descriptor_heap_primary_path =
        descriptor_strategy.uses_descriptor_heap_primary_path && descriptor_heap_plan.backend_ready;
    if !use_descriptor_heap_primary_path {
        destroy_scene_solid_quad_frame_resources(device, frame_resources);
        unsafe {
            device.destroy_swapchain_khr(swapchain, None);
            present_device.device.destroy_device(None);
        }
        return Err(
            "scene sampled-image runtime requires VK_EXT_descriptor_heap; descriptor set and push descriptor paths are disabled"
                .to_owned(),
        );
    }
    let pipeline = match native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_resources(
        device,
        swapchain_plan.format.format,
        swapchain_plan.extent,
        &descriptor_heap_plan,
    ) {
        Ok(pipeline) => pipeline,
        Err(err) => {
            destroy_scene_solid_quad_frame_resources(device, frame_resources);
            unsafe {
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(err);
        }
    };
    let memory_properties =
        unsafe { instance.get_physical_device_memory_properties(selection.physical_device) };
    let native_textures = match scene_sampled_image_load_sources(&sampled_image_sources) {
        Ok(native_textures) => native_textures,
        Err(err) => {
            native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_resources(
                device, pipeline,
            );
            destroy_scene_solid_quad_frame_resources(device, frame_resources);
            unsafe {
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(err);
        }
    };
    let source_extent = native_textures
        .first()
        .map(|texture| vk::Extent2D {
            width: texture.width,
            height: texture.height,
        })
        .unwrap_or(vk::Extent2D {
            width: 0,
            height: 0,
        });
    let geometry_payload = match scene_sampled_image_geometry_payload(
        options.geometry.as_ref(),
        swapchain_plan.extent,
        options.fit,
        source_extent,
        options.scene_size,
        options.scene_fit,
    ) {
        Ok(payload) => payload,
        Err(err) => {
            native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_resources(
                device, pipeline,
            );
            destroy_scene_solid_quad_frame_resources(device, frame_resources);
            unsafe {
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(err);
        }
    };
    let geometry = match create_scene_sampled_image_geometry_resources(
        device,
        &memory_properties,
        geometry_payload,
        frame_resources.in_flight.len(),
    ) {
        Ok(geometry) => geometry,
        Err(err) => {
            native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_resources(
                device, pipeline,
            );
            destroy_scene_solid_quad_frame_resources(device, frame_resources);
            unsafe {
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(err);
        }
    };
    let mut sampled_images = Vec::new();
    for (resource_index, texture) in native_textures.into_iter().enumerate() {
        let resource = match native_vulkan_vulkanalia_create_scene_sampled_image_resources(
            device,
            &memory_properties,
            frame_resources.command_pool,
            present_device.queue,
            scene_sampled_image_resource_sampler_mode(
                resource_index,
                &geometry.draw_steps,
                options.fit,
            ),
            texture.source.display().to_string(),
            &texture,
        ) {
            Ok(resources) => resources,
            Err(err) => {
                for resource in sampled_images.drain(..) {
                    native_vulkan_vulkanalia_destroy_scene_sampled_image_resources(
                        device, resource,
                    );
                }
                destroy_scene_sampled_image_geometry_resources(device, geometry);
                native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_resources(
                    device, pipeline,
                );
                destroy_scene_solid_quad_frame_resources(device, frame_resources);
                unsafe {
                    device.destroy_swapchain_khr(swapchain, None);
                    present_device.device.destroy_device(None);
                }
                return Err(err);
            }
        };
        sampled_images.push(resource);
    }
    native_vulkan_vulkanalia_trim_scene_sampled_image_decode_heap();
    let descriptor_heap = if use_descriptor_heap_primary_path {
        match create_scene_sampled_image_descriptor_heap_resources(
            device,
            &memory_properties,
            &descriptor_heap_plan,
            &sampled_images,
        ) {
            Ok(resources) => Some(resources),
            Err(err) => {
                for resource in sampled_images.drain(..) {
                    native_vulkan_vulkanalia_destroy_scene_sampled_image_resources(
                        device, resource,
                    );
                }
                destroy_scene_sampled_image_geometry_resources(device, geometry);
                native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_resources(
                    device, pipeline,
                );
                destroy_scene_solid_quad_frame_resources(device, frame_resources);
                unsafe {
                    device.destroy_swapchain_khr(swapchain, None);
                    present_device.device.destroy_device(None);
                }
                return Err(err);
            }
        }
    } else {
        None
    };
    let draw_commands =
        match scene_sampled_image_draw_commands(&geometry.draw_steps, &sampled_images) {
            Ok(draw_commands) => draw_commands,
            Err(err) => {
                if let Some(descriptor_heap) = descriptor_heap {
                    native_vulkan_vulkanalia_destroy_descriptor_heap_image_sampler_resources(
                        device,
                        descriptor_heap,
                    );
                }
                for resource in sampled_images.drain(..) {
                    native_vulkan_vulkanalia_destroy_scene_sampled_image_resources(
                        device, resource,
                    );
                }
                destroy_scene_sampled_image_geometry_resources(device, geometry);
                native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_resources(
                    device, pipeline,
                );
                destroy_scene_solid_quad_frame_resources(device, frame_resources);
                unsafe {
                    device.destroy_swapchain_khr(swapchain, None);
                    present_device.device.destroy_device(None);
                }
                return Err(err);
            }
        };
    let solid_pipeline = if options.solid_geometry.is_some() {
        match native_vulkan_vulkanalia_create_scene_solid_quad_pipeline_resources(
            device,
            swapchain_plan.format.format,
            swapchain_plan.extent,
        ) {
            Ok(pipeline) => Some(pipeline),
            Err(err) => {
                if let Some(descriptor_heap) = descriptor_heap {
                    native_vulkan_vulkanalia_destroy_descriptor_heap_image_sampler_resources(
                        device,
                        descriptor_heap,
                    );
                }
                for resource in sampled_images.drain(..) {
                    native_vulkan_vulkanalia_destroy_scene_sampled_image_resources(
                        device, resource,
                    );
                }
                destroy_scene_sampled_image_geometry_resources(device, geometry);
                native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_resources(
                    device, pipeline,
                );
                destroy_scene_solid_quad_frame_resources(device, frame_resources);
                unsafe {
                    device.destroy_swapchain_khr(swapchain, None);
                    present_device.device.destroy_device(None);
                }
                return Err(err);
            }
        }
    } else {
        None
    };
    let solid_geometry = if let Some(solid_geometry_input) = options.solid_geometry.as_ref() {
        let payload = match scene_solid_quad_geometry_payload(
            Some(solid_geometry_input),
            swapchain_plan.extent,
            options.clear_color,
            options.scene_size,
            options.scene_fit,
        ) {
            Ok(payload) => payload,
            Err(err) => {
                if let Some(solid_pipeline) = solid_pipeline {
                    native_vulkan_vulkanalia_destroy_scene_solid_quad_pipeline_resources(
                        device,
                        solid_pipeline,
                    );
                }
                if let Some(descriptor_heap) = descriptor_heap {
                    native_vulkan_vulkanalia_destroy_descriptor_heap_image_sampler_resources(
                        device,
                        descriptor_heap,
                    );
                }
                for resource in sampled_images.drain(..) {
                    native_vulkan_vulkanalia_destroy_scene_sampled_image_resources(
                        device, resource,
                    );
                }
                destroy_scene_sampled_image_geometry_resources(device, geometry);
                native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_resources(
                    device, pipeline,
                );
                destroy_scene_solid_quad_frame_resources(device, frame_resources);
                unsafe {
                    device.destroy_swapchain_khr(swapchain, None);
                    present_device.device.destroy_device(None);
                }
                return Err(err);
            }
        };
        match create_scene_solid_quad_geometry_resources(
            device,
            &memory_properties,
            payload,
            if options.dynamic_solid_geometry.is_some() {
                frame_resources.in_flight.len()
            } else {
                1
            },
        ) {
            Ok(geometry) => Some(geometry),
            Err(err) => {
                if let Some(solid_pipeline) = solid_pipeline {
                    native_vulkan_vulkanalia_destroy_scene_solid_quad_pipeline_resources(
                        device,
                        solid_pipeline,
                    );
                }
                if let Some(descriptor_heap) = descriptor_heap {
                    native_vulkan_vulkanalia_destroy_descriptor_heap_image_sampler_resources(
                        device,
                        descriptor_heap,
                    );
                }
                for resource in sampled_images.drain(..) {
                    native_vulkan_vulkanalia_destroy_scene_sampled_image_resources(
                        device, resource,
                    );
                }
                destroy_scene_sampled_image_geometry_resources(device, geometry);
                native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_resources(
                    device, pipeline,
                );
                destroy_scene_solid_quad_frame_resources(device, frame_resources);
                unsafe {
                    device.destroy_swapchain_khr(swapchain, None);
                    present_device.device.destroy_device(None);
                }
                return Err(err);
            }
        }
    } else {
        None
    };
    let present_timing = VulkanaliaPresentTimingConfig::new(
        swapchain_plan.present_id2_enabled,
        swapchain_plan.present_wait2_enabled,
    );

    let result = run_scene_sampled_image_present_loop(
        vulkan,
        device,
        present_device.queue,
        swapchain,
        &swapchain_images,
        swapchain_plan.extent,
        &frame_resources,
        &pipeline,
        &geometry,
        solid_pipeline.as_ref(),
        solid_geometry.as_ref(),
        &sampled_images,
        &draw_commands,
        descriptor_heap.as_ref(),
        descriptor_strategy,
        &selection,
        &present_device.extension_snapshot,
        &swapchain_plan,
        present_timing,
        options,
    );

    let _ = unsafe { device.device_wait_idle() };
    if let Some(solid_geometry) = solid_geometry {
        destroy_scene_solid_quad_geometry_resources(device, solid_geometry);
    }
    if let Some(solid_pipeline) = solid_pipeline {
        native_vulkan_vulkanalia_destroy_scene_solid_quad_pipeline_resources(
            device,
            solid_pipeline,
        );
    }
    if let Some(descriptor_heap) = descriptor_heap {
        native_vulkan_vulkanalia_destroy_descriptor_heap_image_sampler_resources(
            device,
            descriptor_heap,
        );
    }
    for resource in sampled_images {
        native_vulkan_vulkanalia_destroy_scene_sampled_image_resources(device, resource);
    }
    destroy_scene_sampled_image_geometry_resources(device, geometry);
    native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_resources(device, pipeline);
    destroy_scene_solid_quad_frame_resources(device, frame_resources);
    unsafe {
        device.destroy_swapchain_khr(swapchain, None);
        present_device.device.destroy_device(None);
    }

    result
}

#[allow(clippy::too_many_arguments)]
fn run_scene_solid_quad_present_loop(
    vulkan: &NativeVulkanVulkanaliaInstance,
    device: &Device,
    queue: vk::Queue,
    swapchain: vk::SwapchainKHR,
    swapchain_images: &[vk::Image],
    extent: vk::Extent2D,
    frame_resources: &VulkanaliaSceneSolidQuadFrameResources,
    pipeline: &VulkanaliaSceneSolidQuadPipelineResources,
    geometry: &VulkanaliaSceneSolidQuadGeometryResources,
    selection: &super::swapchain::NativeVulkanVulkanaliaPresentQueueSelection,
    extension_snapshot: &NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot,
    swapchain_plan: &super::swapchain::NativeVulkanVulkanaliaSwapchainPlan,
    present_timing: VulkanaliaPresentTimingConfig,
    options: NativeVulkanVulkanaliaSceneSolidQuadPresentOptions,
) -> Result<NativeVulkanVulkanaliaSceneSolidQuadPresentSnapshot, String> {
    let started_at = Instant::now();
    let deadline = started_at + options.duration;
    let frame_interval = options
        .target_max_fps
        .filter(|fps| *fps > 0)
        .map(|fps| Duration::from_secs_f64(1.0 / fps as f64));
    let mut next_frame = Instant::now();
    let mut frames_presented = 0u64;
    let mut present_ids = Vec::new();
    let mut present_wait_after_present = false;
    let mut last_command = None;

    while Instant::now() < deadline {
        let present_frame_slot = frames_presented as usize % frame_resources.in_flight.len();
        let image_available = frame_resources.image_available[present_frame_slot];
        let render_finished = frame_resources.render_finished[present_frame_slot];
        let in_flight = frame_resources.in_flight[present_frame_slot];
        unsafe {
            device
                .wait_for_fences(&[in_flight], true, u64::MAX)
                .map_err(|err| format!("vkWaitForFences(vulkanalia scene present): {err:?}"))?;
            device
                .reset_fences(&[in_flight])
                .map_err(|err| format!("vkResetFences(vulkanalia scene present): {err:?}"))?;
        }

        let (image_index, _) = unsafe {
            device.acquire_next_image_khr(swapchain, u64::MAX, image_available, vk::Fence::null())
        }
        .map_err(|err| format!("vkAcquireNextImageKHR(vulkanalia scene present): {err:?}"))?;
        let image_index_usize = image_index as usize;
        let command_buffer = frame_resources
            .command_buffers
            .get(image_index_usize)
            .copied()
            .ok_or_else(|| {
                format!("swapchain image index {image_index_usize} has no command buffer")
            })?;
        let swapchain_image = *swapchain_images
            .get(image_index_usize)
            .ok_or_else(|| format!("swapchain image index {image_index_usize} is unavailable"))?;
        let swapchain_view = *frame_resources
            .swapchain_image_views
            .get(image_index_usize)
            .ok_or_else(|| format!("swapchain view index {image_index_usize} is unavailable"))?;

        let elapsed_ms = started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        let dynamic_payload = options
            .dynamic_geometry
            .as_ref()
            .map(|dynamic_geometry| {
                let input = dynamic_geometry(elapsed_ms)?;
                scene_solid_quad_geometry_payload(
                    Some(&input),
                    extent,
                    options.quad_color,
                    options.scene_size,
                    options.scene_fit,
                )
            })
            .transpose()?;
        let vertex_buffer = update_scene_solid_quad_geometry_for_time(
            device,
            geometry,
            present_frame_slot,
            dynamic_payload.as_ref(),
        )?;

        let command = native_vulkan_vulkanalia_record_scene_solid_quad_command_buffer(
            device,
            command_buffer,
            swapchain_image,
            swapchain_view,
            extent,
            pipeline,
            vertex_buffer,
            geometry.index_buffer,
            geometry.snapshot.index_count,
            [
                options.quad_color.r,
                options.quad_color.g,
                options.quad_color.b,
                options.quad_color.a,
            ],
        )?;
        submit_scene_solid_quad_command_buffer2(
            device,
            queue,
            command_buffer,
            image_available,
            render_finished,
            in_flight,
        )?;

        let swapchains = [swapchain];
        let image_indices = [image_index];
        let wait_semaphores = [render_finished];
        let present_id = present_timing.present_id(frames_presented as u32);
        let present_id_values = [present_id.unwrap_or(0)];
        let mut present_id2_info = present_id.map(|_| {
            vk::PresentId2KHR::builder()
                .present_ids(&present_id_values)
                .build()
        });
        let mut present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(&wait_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);
        if present_timing.present_id2_enabled {
            if let Some(present_id2_info) = present_id2_info.as_mut() {
                present_info = present_info.push_next(present_id2_info);
            }
        }
        unsafe {
            device
                .queue_present_khr(queue, &present_info)
                .map_err(|err| format!("vkQueuePresentKHR(vulkanalia scene present): {err:?}"))?;
        }
        present_wait_after_present |= present_timing.wait_after_queue_present(
            device,
            swapchain,
            present_id,
            "scene solid quad present",
        )?;

        present_ids.push(present_id);
        frames_presented += 1;
        last_command = Some(command);

        if let Some(interval) = frame_interval {
            next_frame += interval;
            let now = Instant::now();
            if next_frame > now {
                thread::sleep(next_frame - now);
            } else {
                next_frame = now;
            }
        }
    }

    let elapsed = started_at.elapsed();
    Ok(NativeVulkanVulkanaliaSceneSolidQuadPresentSnapshot {
        binding: "vulkanalia",
        route: "scene-solid-quad-visible-present",
        loader: vulkan.loader_name.to_owned(),
        requested_api_version: Version::V1_4_0.to_string(),
        runtime_elapsed_ms: elapsed.as_millis().min(u64::MAX as u128) as u64,
        frames_presented,
        average_present_fps: if elapsed.is_zero() {
            0.0
        } else {
            frames_presented as f64 / elapsed.as_secs_f64()
        },
        quad_color: options.quad_color,
        selected_queue: NativeVulkanVulkanaliaPresentQueueSnapshot {
            physical_device_index: selection.physical_device_index,
            physical_device_name: selection.physical_device_name.clone(),
            physical_device_type: selection.physical_device_type.clone(),
            queue_family_index: selection.queue_family_index,
            queue_count: selection.queue_count,
            queue_flags: queue_flag_labels(selection.queue_flags),
            supports_graphics: selection.queue_flags.contains(vk::QueueFlags::GRAPHICS),
            supports_present: true,
            supports_wayland_presentation: selection.supports_wayland_presentation,
        },
        device_extensions: extension_snapshot.clone(),
        swapchain: NativeVulkanVulkanaliaSwapchainSnapshot {
            created: true,
            format: format!("{:?}", swapchain_plan.format.format),
            color_space: format!("{:?}", swapchain_plan.format.color_space),
            present_mode: present_mode_label(swapchain_plan.present_mode),
            extent: (swapchain_plan.extent.width, swapchain_plan.extent.height),
            image_count: swapchain_images.len(),
            min_image_count: swapchain_plan.image_count,
            composite_alpha: composite_alpha_label(swapchain_plan.composite_alpha),
            image_usage: vec!["transfer-dst", "color-attachment"],
            create_flags: swapchain_create_flag_labels(swapchain_plan.create_flags),
            present_id2_enabled: swapchain_plan.present_id2_enabled,
            present_wait2_enabled: swapchain_plan.present_wait2_enabled,
        },
        geometry: geometry.snapshot.clone(),
        pipeline: pipeline.snapshot.clone(),
        last_command,
        command_submit_model: if present_wait_after_present {
            "acquire_next_image_khr -> cmd_begin_rendering solid quad -> queue_submit2 -> queue_present_khr -> wait_for_present"
        } else {
            "acquire_next_image_khr -> cmd_begin_rendering solid quad -> queue_submit2 -> queue_present_khr"
        },
        present_sync_model: "frame-slot semaphore/fence reuse; no per-present queue_wait_idle",
        wait_idle_after_present: false,
        present_ids,
        uses_present_id2: present_timing.present_id2_enabled,
        present_wait2_available: present_timing.present_wait2_enabled,
        present_wait_after_present,
        uses_pipeline_rendering_create_info: true,
        uses_dynamic_rendering: true,
        uses_synchronization2: true,
        uses_submit2: true,
        zero_copy_scope: "scene geometry is retained in Vulkan buffers and rendered directly to the swapchain",
        primary_reference: "Vulkan dynamic rendering; FFmpeg remains first reference for clock/queue discipline",
    })
}

#[allow(clippy::too_many_arguments)]
fn run_scene_sampled_image_present_loop(
    vulkan: &NativeVulkanVulkanaliaInstance,
    device: &Device,
    queue: vk::Queue,
    swapchain: vk::SwapchainKHR,
    swapchain_images: &[vk::Image],
    extent: vk::Extent2D,
    frame_resources: &VulkanaliaSceneSolidQuadFrameResources,
    pipeline: &VulkanaliaSceneSampledImagePipelineResources,
    geometry: &VulkanaliaSceneSampledImageGeometryResources,
    solid_pipeline: Option<&VulkanaliaSceneSolidQuadPipelineResources>,
    solid_geometry: Option<&VulkanaliaSceneSolidQuadGeometryResources>,
    sampled_images: &[VulkanaliaSceneSampledImageResources],
    draw_commands: &[VulkanaliaSceneSampledImageDrawCommand],
    descriptor_heap: Option<&VulkanaliaDescriptorHeapImageSamplerResources>,
    descriptor_strategy: NativeVulkanVulkanaliaSceneSampledImageDescriptorStrategySnapshot,
    selection: &super::swapchain::NativeVulkanVulkanaliaPresentQueueSelection,
    extension_snapshot: &NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot,
    swapchain_plan: &super::swapchain::NativeVulkanVulkanaliaSwapchainPlan,
    present_timing: VulkanaliaPresentTimingConfig,
    options: NativeVulkanVulkanaliaSceneSampledImagePresentOptions,
) -> Result<NativeVulkanVulkanaliaSceneSampledImagePresentSnapshot, String> {
    let started_at = Instant::now();
    let deadline = started_at + options.duration;
    let frame_interval = options
        .target_max_fps
        .filter(|fps| *fps > 0)
        .map(|fps| Duration::from_secs_f64(1.0 / fps as f64));
    let mut next_frame = Instant::now();
    let mut frames_presented = 0u64;
    let mut present_ids = Vec::new();
    let mut present_wait_after_present = false;
    let mut last_command = None;
    let sampled_image = sampled_images.first().ok_or_else(|| {
        "scene sampled image present requires at least one sampled image".to_owned()
    })?;
    let solid_draw_commands = match solid_geometry {
        Some(geometry) => Some(scene_solid_quad_draw_commands(&geometry.draw_steps)?),
        None => None,
    };
    let solid_quad_draw = match (
        solid_pipeline,
        solid_geometry,
        solid_draw_commands.as_deref(),
    ) {
        (Some(pipeline_resources), Some(geometry), Some(draw_commands)) => {
            let vertex_buffer = geometry
                .vertex_buffers
                .first()
                .ok_or_else(|| "scene mixed solid geometry has no vertex buffers".to_owned())?
                .buffer;
            Some(VulkanaliaSceneSolidQuadDrawResources {
                pipeline_resources,
                vertex_buffer,
                index_buffer: geometry.index_buffer,
                draw_commands,
            })
        }
        (None, None, None) => None,
        _ => {
            return Err(
                "scene mixed present requires both solid pipeline and solid geometry".to_owned(),
            );
        }
    };
    let descriptor_heap_draw =
        descriptor_heap.map(|resources| VulkanaliaSceneDescriptorHeapDrawResources { resources });

    while Instant::now() < deadline {
        let present_frame_slot = frames_presented as usize % frame_resources.in_flight.len();
        let image_available = frame_resources.image_available[present_frame_slot];
        let render_finished = frame_resources.render_finished[present_frame_slot];
        let in_flight = frame_resources.in_flight[present_frame_slot];
        unsafe {
            device
                .wait_for_fences(&[in_flight], true, u64::MAX)
                .map_err(|err| {
                    format!("vkWaitForFences(vulkanalia scene sampled image present): {err:?}")
                })?;
            device.reset_fences(&[in_flight]).map_err(|err| {
                format!("vkResetFences(vulkanalia scene sampled image present): {err:?}")
            })?;
        }

        let (image_index, _) = unsafe {
            device.acquire_next_image_khr(swapchain, u64::MAX, image_available, vk::Fence::null())
        }
        .map_err(|err| {
            format!("vkAcquireNextImageKHR(vulkanalia scene sampled image present): {err:?}")
        })?;
        let image_index_usize = image_index as usize;
        let command_buffer = frame_resources
            .command_buffers
            .get(image_index_usize)
            .copied()
            .ok_or_else(|| {
                format!("swapchain image index {image_index_usize} has no command buffer")
            })?;
        let swapchain_image = *swapchain_images
            .get(image_index_usize)
            .ok_or_else(|| format!("swapchain image index {image_index_usize} is unavailable"))?;
        let swapchain_view = *frame_resources
            .swapchain_image_views
            .get(image_index_usize)
            .ok_or_else(|| format!("swapchain view index {image_index_usize} is unavailable"))?;

        let elapsed_ms = started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
        let dynamic_payload = options
            .dynamic_geometry
            .as_ref()
            .map(|dynamic_geometry| {
                let input = dynamic_geometry(elapsed_ms)?;
                scene_sampled_image_geometry_payload(
                    Some(&input),
                    extent,
                    options.fit,
                    vk::Extent2D {
                        width: 0,
                        height: 0,
                    },
                    options.scene_size,
                    options.scene_fit,
                )
            })
            .transpose()?;
        let vertex_buffer = update_scene_sampled_image_geometry_for_time(
            device,
            geometry,
            present_frame_slot,
            elapsed_ms,
            dynamic_payload.as_ref(),
        )?;
        let dynamic_solid_payload = options
            .dynamic_solid_geometry
            .as_ref()
            .map(|dynamic_geometry| {
                dynamic_geometry(elapsed_ms)?
                    .map(|input| {
                        scene_solid_quad_geometry_payload(
                            Some(&input),
                            extent,
                            options.clear_color,
                            options.scene_size,
                            options.scene_fit,
                        )
                    })
                    .transpose()
            })
            .transpose()?
            .flatten();
        let solid_quad_draw = match (solid_quad_draw.as_ref(), solid_geometry) {
            (Some(draw), Some(geometry)) => {
                let solid_vertex_buffer = update_scene_solid_quad_geometry_for_time(
                    device,
                    geometry,
                    present_frame_slot,
                    dynamic_solid_payload.as_ref(),
                )?;
                Some(VulkanaliaSceneSolidQuadDrawResources {
                    pipeline_resources: draw.pipeline_resources,
                    vertex_buffer: solid_vertex_buffer,
                    index_buffer: draw.index_buffer,
                    draw_commands: draw.draw_commands,
                })
            }
            _ => None,
        };

        let command = native_vulkan_vulkanalia_record_scene_sampled_image_command_buffer(
            device,
            command_buffer,
            swapchain_image,
            swapchain_view,
            extent,
            solid_quad_draw,
            descriptor_heap_draw,
            pipeline,
            draw_commands,
            vertex_buffer,
            geometry.index_buffer,
            [
                options.clear_color.r,
                options.clear_color.g,
                options.clear_color.b,
                options.clear_color.a,
            ],
        )?;
        submit_scene_solid_quad_command_buffer2(
            device,
            queue,
            command_buffer,
            image_available,
            render_finished,
            in_flight,
        )?;

        let swapchains = [swapchain];
        let image_indices = [image_index];
        let wait_semaphores = [render_finished];
        let present_id = present_timing.present_id(frames_presented as u32);
        let present_id_values = [present_id.unwrap_or(0)];
        let mut present_id2_info = present_id.map(|_| {
            vk::PresentId2KHR::builder()
                .present_ids(&present_id_values)
                .build()
        });
        let mut present_info = vk::PresentInfoKHR::builder()
            .wait_semaphores(&wait_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);
        if present_timing.present_id2_enabled {
            if let Some(present_id2_info) = present_id2_info.as_mut() {
                present_info = present_info.push_next(present_id2_info);
            }
        }
        unsafe {
            device
                .queue_present_khr(queue, &present_info)
                .map_err(|err| {
                    format!("vkQueuePresentKHR(vulkanalia scene sampled image present): {err:?}")
                })?;
        }
        present_wait_after_present |= present_timing.wait_after_queue_present(
            device,
            swapchain,
            present_id,
            "scene sampled image present",
        )?;

        present_ids.push(present_id);
        frames_presented += 1;
        last_command = Some(command);

        if let Some(interval) = frame_interval {
            next_frame += interval;
            let now = Instant::now();
            if next_frame > now {
                thread::sleep(next_frame - now);
            } else {
                next_frame = now;
            }
        }
    }

    let elapsed = started_at.elapsed();
    Ok(NativeVulkanVulkanaliaSceneSampledImagePresentSnapshot {
        binding: "vulkanalia",
        route: "scene-sampled-image-visible-present",
        scene_input_model: "core scene snapshot layers; groups must be flattened before native Vulkan planning",
        scene_resource_model: if solid_quad_draw.is_some() {
            "retained-solid-quad-geometry-and-sampled-images-descriptor-heap"
        } else {
            "retained-sampled-images-descriptor-heap"
        },
        scene_solid_quad_draw_count: solid_geometry
            .map(|geometry| geometry.snapshot.draw_step_count)
            .unwrap_or(0),
        scene_sampled_image_resource_count: sampled_images.len().min(u32::MAX as usize) as u32,
        scene_sampled_image_descriptor_heap_required: true,
        loader: vulkan.loader_name.to_owned(),
        requested_api_version: Version::V1_4_0.to_string(),
        runtime_elapsed_ms: elapsed.as_millis().min(u64::MAX as u128) as u64,
        frames_presented,
        average_present_fps: if elapsed.is_zero() {
            0.0
        } else {
            frames_presented as f64 / elapsed.as_secs_f64()
        },
        source: options.source,
        clear_color: options.clear_color,
        fit: options.fit,
        mixed_scene_draw_enabled: solid_quad_draw.is_some(),
        selected_queue: NativeVulkanVulkanaliaPresentQueueSnapshot {
            physical_device_index: selection.physical_device_index,
            physical_device_name: selection.physical_device_name.clone(),
            physical_device_type: selection.physical_device_type.clone(),
            queue_family_index: selection.queue_family_index,
            queue_count: selection.queue_count,
            queue_flags: queue_flag_labels(selection.queue_flags),
            supports_graphics: selection.queue_flags.contains(vk::QueueFlags::GRAPHICS),
            supports_present: true,
            supports_wayland_presentation: selection.supports_wayland_presentation,
        },
        device_extensions: extension_snapshot.clone(),
        swapchain: NativeVulkanVulkanaliaSwapchainSnapshot {
            created: true,
            format: format!("{:?}", swapchain_plan.format.format),
            color_space: format!("{:?}", swapchain_plan.format.color_space),
            present_mode: present_mode_label(swapchain_plan.present_mode),
            extent: (swapchain_plan.extent.width, swapchain_plan.extent.height),
            image_count: swapchain_images.len(),
            min_image_count: swapchain_plan.image_count,
            composite_alpha: composite_alpha_label(swapchain_plan.composite_alpha),
            image_usage: vec!["transfer-dst", "color-attachment"],
            create_flags: swapchain_create_flag_labels(swapchain_plan.create_flags),
            present_id2_enabled: swapchain_plan.present_id2_enabled,
            present_wait2_enabled: swapchain_plan.present_wait2_enabled,
        },
        solid_geometry: solid_geometry.map(|geometry| geometry.snapshot.clone()),
        solid_pipeline: solid_pipeline.map(|pipeline| pipeline.snapshot.clone()),
        geometry: geometry.snapshot.clone(),
        sampled_image: sampled_image.snapshot.clone(),
        sampled_images: sampled_images
            .iter()
            .map(|resource| resource.snapshot.clone())
            .collect(),
        descriptor_strategy,
        descriptor_heap: descriptor_heap.map(|resources| resources.snapshot.clone()),
        pipeline: pipeline.snapshot.clone(),
        last_command,
        command_submit_model: scene_present_submit_model(
            solid_quad_draw.is_some(),
            present_wait_after_present,
        ),
        present_sync_model: "frame-slot semaphore/fence reuse; no per-present queue_wait_idle",
        wait_idle_after_present: false,
        present_ids,
        uses_present_id2: present_timing.present_id2_enabled,
        present_wait2_available: present_timing.present_wait2_enabled,
        present_wait_after_present,
        uses_pipeline_rendering_create_info: true,
        uses_dynamic_rendering: true,
        uses_synchronization2: true,
        uses_submit2: true,
        zero_copy_scope: if solid_quad_draw.is_some() {
            "scene geometry buffers and retained sampled images render directly to the swapchain; no scene snapshot upload"
        } else {
            "source image is uploaded once into a retained Vulkan sampled image and rendered directly to the swapchain"
        },
        primary_reference: "Vulkan dynamic rendering and sync2; FFmpeg frame lifetime discipline for retained resources",
    })
}

fn scene_present_submit_model(
    includes_solid_quads: bool,
    present_wait_after_present: bool,
) -> &'static str {
    match (includes_solid_quads, present_wait_after_present) {
        (true, true) => {
            "acquire_next_image_khr -> cmd_begin_rendering solid quads then sampled image quads -> queue_submit2 -> queue_present_khr -> wait_for_present"
        }
        (true, false) => {
            "acquire_next_image_khr -> cmd_begin_rendering solid quads then sampled image quads -> queue_submit2 -> queue_present_khr"
        }
        (false, true) => {
            "acquire_next_image_khr -> cmd_begin_rendering sampled image quad -> queue_submit2 -> queue_present_khr -> wait_for_present"
        }
        (false, false) => {
            "acquire_next_image_khr -> cmd_begin_rendering sampled image quad -> queue_submit2 -> queue_present_khr"
        }
    }
}

fn create_scene_solid_quad_frame_resources(
    device: &Device,
    swapchain_images: &[vk::Image],
    swapchain_format: vk::Format,
    queue_family_index: u32,
) -> Result<VulkanaliaSceneSolidQuadFrameResources, String> {
    if swapchain_images.is_empty() {
        return Err("scene present requires at least one swapchain image".to_owned());
    }

    let mut swapchain_image_views = Vec::new();
    let mut command_pool = vk::CommandPool::null();
    let mut image_available = Vec::new();
    let mut render_finished = Vec::new();
    let mut in_flight = Vec::new();

    let result = (|| -> Result<VulkanaliaSceneSolidQuadFrameResources, String> {
        swapchain_image_views = create_scene_solid_quad_swapchain_image_views(
            device,
            swapchain_images,
            swapchain_format,
        )?;

        let command_pool_info = vk::CommandPoolCreateInfo::builder()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(queue_family_index);
        command_pool = unsafe { device.create_command_pool(&command_pool_info, None) }
            .map_err(|err| format!("vkCreateCommandPool(vulkanalia scene present): {err:?}"))?;
        let command_buffer_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(swapchain_images.len() as u32);
        let command_buffers = unsafe { device.allocate_command_buffers(&command_buffer_info) }
            .map_err(|err| {
                format!("vkAllocateCommandBuffers(vulkanalia scene present): {err:?}")
            })?;

        let semaphore_info = vk::SemaphoreCreateInfo::builder();
        let fence_info = vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);
        for frame_slot in 0..swapchain_images.len() {
            image_available.push(
                unsafe { device.create_semaphore(&semaphore_info, None) }.map_err(|err| {
                    format!(
                        "vkCreateSemaphore(image_available slot {frame_slot} vulkanalia scene present): {err:?}"
                    )
                })?,
            );
            render_finished.push(
                unsafe { device.create_semaphore(&semaphore_info, None) }.map_err(|err| {
                    format!(
                        "vkCreateSemaphore(render_finished slot {frame_slot} vulkanalia scene present): {err:?}"
                    )
                })?,
            );
            in_flight.push(
                unsafe { device.create_fence(&fence_info, None) }.map_err(|err| {
                    format!("vkCreateFence(slot {frame_slot} vulkanalia scene present): {err:?}")
                })?,
            );
        }

        Ok(VulkanaliaSceneSolidQuadFrameResources {
            swapchain_image_views: std::mem::take(&mut swapchain_image_views),
            command_pool,
            command_buffers,
            image_available: std::mem::take(&mut image_available),
            render_finished: std::mem::take(&mut render_finished),
            in_flight: std::mem::take(&mut in_flight),
        })
    })();

    if result.is_err() {
        destroy_partial_scene_solid_quad_frame_resources(
            device,
            swapchain_image_views,
            command_pool,
            image_available,
            render_finished,
            in_flight,
        );
    }

    result
}

fn create_scene_solid_quad_swapchain_image_views(
    device: &Device,
    images: &[vk::Image],
    format: vk::Format,
) -> Result<Vec<vk::ImageView>, String> {
    let mut views = Vec::with_capacity(images.len());
    for image in images {
        let create_info = vk::ImageViewCreateInfo::builder()
            .image(*image)
            .view_type(vk::ImageViewType::_2D)
            .format(format)
            .subresource_range(scene_color_subresource_range());
        match unsafe { device.create_image_view(&create_info, None) } {
            Ok(view) => views.push(view),
            Err(err) => {
                for view in views {
                    unsafe {
                        device.destroy_image_view(view, None);
                    }
                }
                return Err(format!(
                    "vkCreateImageView(vulkanalia scene present swapchain): {err:?}"
                ));
            }
        }
    }
    Ok(views)
}

fn destroy_scene_solid_quad_frame_resources(
    device: &Device,
    resources: VulkanaliaSceneSolidQuadFrameResources,
) {
    destroy_partial_scene_solid_quad_frame_resources(
        device,
        resources.swapchain_image_views,
        resources.command_pool,
        resources.image_available,
        resources.render_finished,
        resources.in_flight,
    );
}

fn destroy_partial_scene_solid_quad_frame_resources(
    device: &Device,
    swapchain_image_views: Vec<vk::ImageView>,
    command_pool: vk::CommandPool,
    image_available: Vec<vk::Semaphore>,
    render_finished: Vec<vk::Semaphore>,
    in_flight: Vec<vk::Fence>,
) {
    unsafe {
        for fence in in_flight {
            if fence != vk::Fence::null() {
                device.destroy_fence(fence, None);
            }
        }
        for semaphore in render_finished {
            if semaphore != vk::Semaphore::null() {
                device.destroy_semaphore(semaphore, None);
            }
        }
        for semaphore in image_available {
            if semaphore != vk::Semaphore::null() {
                device.destroy_semaphore(semaphore, None);
            }
        }
        if command_pool != vk::CommandPool::null() {
            device.destroy_command_pool(command_pool, None);
        }
        for view in swapchain_image_views {
            device.destroy_image_view(view, None);
        }
    }
}

fn create_scene_solid_quad_geometry_resources(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    payload: VulkanaliaSceneSolidQuadGeometryPayload,
    vertex_buffer_count: usize,
) -> Result<VulkanaliaSceneSolidQuadGeometryResources, String> {
    let vertex_buffer_count = vertex_buffer_count.max(1);
    let mut vertex_buffers = Vec::with_capacity(vertex_buffer_count);
    for vertex_buffer_index in 0..vertex_buffer_count {
        match create_scene_uploaded_buffer(
            device,
            memory_properties,
            &payload.vertex_bytes,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            if vertex_buffer_count > 1 {
                "solid-quad per-frame vertex"
            } else {
                "solid-quad vertex"
            },
        ) {
            Ok(vertex) => vertex_buffers.push(vertex),
            Err(err) => {
                for vertex in vertex_buffers {
                    destroy_scene_uploaded_buffer(device, vertex);
                }
                return Err(format!(
                    "create solid-quad vertex buffer slot {vertex_buffer_index}: {err}"
                ));
            }
        }
    }
    let index = match create_scene_uploaded_buffer(
        device,
        memory_properties,
        &payload.index_bytes,
        vk::BufferUsageFlags::INDEX_BUFFER,
        "solid-quad index",
    ) {
        Ok(index) => index,
        Err(err) => {
            for vertex in vertex_buffers {
                destroy_scene_uploaded_buffer(device, vertex);
            }
            return Err(err);
        }
    };
    let first_vertex = vertex_buffers
        .first()
        .ok_or_else(|| "scene solid-quad geometry created no vertex buffers".to_owned())?;
    let selected_vertex_memory_type_index = first_vertex.memory_type.index;
    let vertex_memory_property_flags =
        memory_property_flag_labels(first_vertex.memory_type.property_flags_bits);

    Ok(VulkanaliaSceneSolidQuadGeometryResources {
        vertex_buffers,
        index_buffer: index.buffer,
        index_memory: index.memory,
        draw_steps: payload.draw_steps.clone(),
        indices: payload.indices,
        snapshot: NativeVulkanVulkanaliaSceneSolidQuadGeometrySnapshot {
            source_label: payload.source_label,
            vertex_count: payload.vertex_count,
            vertex_buffer_bytes: payload.vertex_bytes.len() as u64,
            index_buffer_bytes: payload.index_bytes.len() as u64,
            index_count: payload.index_count,
            quad_count: payload.quad_count,
            draw_step_count: payload.draw_steps.len().min(u32::MAX as usize) as u32,
            vertex_stride_bytes: SCENE_FULL_SOLID_QUAD_VERTEX_STRIDE_BYTES,
            selected_vertex_memory_type_index,
            selected_index_memory_type_index: index.memory_type.index,
            vertex_memory_property_flags,
            index_memory_property_flags: memory_property_flag_labels(
                index.memory_type.property_flags_bits,
            ),
            upload_model: if vertex_buffer_count > 1 {
                "per-frame host-visible solid-quad geometry buffers retained across present frames"
            } else {
                "one-time host-visible geometry upload retained across present frames"
            },
            retained_across_frames: true,
        },
    })
}

fn destroy_scene_solid_quad_geometry_resources(
    device: &Device,
    resources: VulkanaliaSceneSolidQuadGeometryResources,
) {
    unsafe {
        device.destroy_buffer(resources.index_buffer, None);
        device.free_memory(resources.index_memory, None);
    }
    for vertex in resources.vertex_buffers {
        destroy_scene_uploaded_buffer(device, vertex);
    }
}

fn update_scene_solid_quad_geometry_for_time(
    device: &Device,
    geometry: &VulkanaliaSceneSolidQuadGeometryResources,
    frame_slot: usize,
    dynamic_payload: Option<&VulkanaliaSceneSolidQuadGeometryPayload>,
) -> Result<vk::Buffer, String> {
    if geometry.vertex_buffers.is_empty() {
        return Err("scene solid-quad geometry has no vertex buffers".to_owned());
    }
    let vertex = geometry
        .vertex_buffers
        .get(frame_slot % geometry.vertex_buffers.len())
        .expect("scene solid-quad vertex buffer checked non-empty");
    let Some(payload) = dynamic_payload else {
        return Ok(vertex.buffer);
    };
    if payload.indices != geometry.indices {
        return Err("scene dynamic solid-quad geometry changed index topology".to_owned());
    }
    if payload.draw_steps != geometry.draw_steps {
        return Err("scene dynamic solid-quad geometry changed draw step topology".to_owned());
    }
    let expected_bytes = geometry.snapshot.vertex_buffer_bytes as usize;
    if payload.vertex_bytes.len() != expected_bytes {
        return Err(format!(
            "scene dynamic solid-quad vertex bytes {} did not match retained buffer bytes {}",
            payload.vertex_bytes.len(),
            expected_bytes
        ));
    }
    write_scene_uploaded_buffer(
        device,
        vertex,
        &payload.vertex_bytes,
        "dynamic scene solid-quad vertex",
    )?;
    Ok(vertex.buffer)
}

fn create_scene_sampled_image_geometry_resources(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    payload: VulkanaliaSceneSampledImageGeometryPayload,
    frame_resource_count: usize,
) -> Result<VulkanaliaSceneSampledImageGeometryResources, String> {
    let animated_geometry = scene_sampled_image_draw_steps_are_animated(&payload.draw_steps);
    let vertex_buffer_count =
        scene_sampled_image_vertex_buffer_count(&payload.draw_steps, frame_resource_count);
    let mut vertex_buffers = Vec::with_capacity(vertex_buffer_count);
    for vertex_buffer_index in 0..vertex_buffer_count {
        match create_scene_uploaded_buffer(
            device,
            memory_properties,
            &payload.vertex_bytes,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            if animated_geometry {
                "sampled-image per-frame vertex"
            } else {
                "sampled-image vertex"
            },
        ) {
            Ok(vertex) => vertex_buffers.push(vertex),
            Err(err) => {
                for vertex in vertex_buffers {
                    destroy_scene_uploaded_buffer(device, vertex);
                }
                return Err(format!(
                    "create sampled-image vertex buffer slot {vertex_buffer_index}: {err}"
                ));
            }
        }
    }
    let index = match create_scene_uploaded_buffer(
        device,
        memory_properties,
        &payload.index_bytes,
        vk::BufferUsageFlags::INDEX_BUFFER,
        "sampled-image index",
    ) {
        Ok(index) => index,
        Err(err) => {
            for vertex in vertex_buffers {
                destroy_scene_uploaded_buffer(device, vertex);
            }
            return Err(err);
        }
    };
    let first_vertex = vertex_buffers
        .first()
        .ok_or_else(|| "scene sampled-image geometry created no vertex buffers".to_owned())?;
    let selected_vertex_memory_type_index = first_vertex.memory_type.index;
    let vertex_memory_property_flags =
        memory_property_flag_labels(first_vertex.memory_type.property_flags_bits);

    Ok(VulkanaliaSceneSampledImageGeometryResources {
        vertex_buffers,
        index_buffer: index.buffer,
        index_memory: index.memory,
        draw_steps: payload.draw_steps.clone(),
        base_vertices: payload.vertices,
        indices: payload.indices,
        sources: payload.sources,
        snapshot: NativeVulkanVulkanaliaSceneSampledImageGeometrySnapshot {
            source_label: payload.source_label,
            vertex_count: payload.vertex_count,
            vertex_buffer_bytes: payload.vertex_bytes.len() as u64,
            vertex_buffer_count: vertex_buffer_count.min(u32::MAX as usize) as u32,
            index_buffer_bytes: payload.index_bytes.len() as u64,
            index_count: payload.index_count,
            quad_count: payload.quad_count,
            source_count: payload.source_count,
            draw_step_count: payload.draw_steps.len().min(u32::MAX as usize) as u32,
            vertex_stride_bytes: SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES,
            selected_vertex_memory_type_index,
            selected_index_memory_type_index: index.memory_type.index,
            vertex_memory_property_flags,
            index_memory_property_flags: memory_property_flag_labels(
                index.memory_type.property_flags_bits,
            ),
            upload_model: if animated_geometry {
                "per-frame host-visible sampled-image geometry buffers retained across present frames"
            } else {
                "one-time host-visible sampled-image geometry upload retained across present frames"
            },
            retained_across_frames: true,
        },
    })
}

fn destroy_scene_sampled_image_geometry_resources(
    device: &Device,
    resources: VulkanaliaSceneSampledImageGeometryResources,
) {
    unsafe {
        device.destroy_buffer(resources.index_buffer, None);
        device.free_memory(resources.index_memory, None);
    }
    for vertex in resources.vertex_buffers {
        destroy_scene_uploaded_buffer(device, vertex);
    }
}

fn write_scene_uploaded_buffer(
    device: &Device,
    buffer: &VulkanaliaSceneUploadedBuffer,
    bytes: &[u8],
    label: &'static str,
) -> Result<(), String> {
    let map = native_vulkan_vulkanalia_map_memory2(
        device,
        buffer.memory,
        0,
        bytes.len() as u64,
        vk::MemoryMapFlags::empty(),
        label,
    )?;
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), map.cast::<u8>(), bytes.len());
    }
    let host_coherent = buffer.memory_type.property_flags_bits
        & vk::MemoryPropertyFlags::HOST_COHERENT.bits()
        == vk::MemoryPropertyFlags::HOST_COHERENT.bits();
    if !host_coherent {
        let range = vk::MappedMemoryRange::builder()
            .memory(buffer.memory)
            .offset(0)
            .size(vk::WHOLE_SIZE)
            .build();
        if let Err(err) = unsafe { device.flush_mapped_memory_ranges(&[range]) } {
            let _ = native_vulkan_vulkanalia_unmap_memory2(device, buffer.memory, label);
            return Err(format!(
                "vkFlushMappedMemoryRanges(vulkanalia {label}): {err:?}"
            ));
        }
    }
    native_vulkan_vulkanalia_unmap_memory2(device, buffer.memory, label)
}

fn update_scene_sampled_image_geometry_for_time(
    device: &Device,
    geometry: &VulkanaliaSceneSampledImageGeometryResources,
    frame_slot: usize,
    elapsed_ms: u64,
    dynamic_payload: Option<&VulkanaliaSceneSampledImageGeometryPayload>,
) -> Result<vk::Buffer, String> {
    if geometry.vertex_buffers.is_empty() {
        return Err("scene sampled-image geometry has no vertex buffers".to_owned());
    }
    let vertex = geometry
        .vertex_buffers
        .get(frame_slot % geometry.vertex_buffers.len())
        .expect("scene sampled-image vertex buffer checked non-empty");
    if dynamic_payload.is_none() && !scene_sampled_image_geometry_is_animated(geometry) {
        return Ok(vertex.buffer);
    }
    let (mut vertices, draw_steps) = if let Some(payload) = dynamic_payload {
        if payload.indices != geometry.indices {
            return Err("scene dynamic sampled-image geometry changed index topology".to_owned());
        }
        if payload.sources != geometry.sources {
            return Err("scene dynamic sampled-image geometry changed sampled sources".to_owned());
        }
        if !scene_sampled_image_draw_step_topology_matches(
            &payload.draw_steps,
            &geometry.draw_steps,
        ) {
            return Err(
                "scene dynamic sampled-image geometry changed draw step topology".to_owned(),
            );
        }
        (payload.vertices.clone(), payload.draw_steps.as_slice())
    } else {
        (
            geometry.base_vertices.clone(),
            geometry.draw_steps.as_slice(),
        )
    };
    if scene_sampled_image_draw_steps_are_animated(draw_steps) {
        native_vulkan_scene_apply_elapsed_texture_regions(
            &mut vertices,
            &geometry.indices,
            draw_steps,
            elapsed_ms,
        )?;
    }
    let vertex_bytes = scene_sampled_image_vertex_bytes(&vertices)?;
    let expected_bytes = geometry.snapshot.vertex_buffer_bytes as usize;
    if vertex_bytes.len() != expected_bytes {
        return Err(format!(
            "scene sampled-image animated vertex bytes {} did not match retained buffer bytes {}",
            vertex_bytes.len(),
            expected_bytes
        ));
    }
    write_scene_uploaded_buffer(
        device,
        vertex,
        &vertex_bytes,
        "animated scene sampled-image vertex",
    )?;
    Ok(vertex.buffer)
}

fn scene_sampled_image_geometry_is_animated(
    geometry: &VulkanaliaSceneSampledImageGeometryResources,
) -> bool {
    scene_sampled_image_draw_steps_are_animated(&geometry.draw_steps)
}

fn scene_sampled_image_draw_step_topology_matches(
    left: &[NativeVulkanVulkanaliaSceneSampledImageDrawStep],
    right: &[NativeVulkanVulkanaliaSceneSampledImageDrawStep],
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.layer_index == right.layer_index
                && left.resource_index == right.resource_index
                && left.first_index == right.first_index
                && left.index_count == right.index_count
                && left.fit == right.fit
        })
}

fn native_vulkan_scene_apply_elapsed_texture_regions(
    vertices: &mut [NativeVulkanVulkanaliaSceneSampledImageVertex],
    geometry_indices: &[u32],
    draw_steps: &[NativeVulkanVulkanaliaSceneSampledImageDrawStep],
    elapsed_ms: u64,
) -> Result<(), String> {
    for step in draw_steps {
        let Some(region) = step.texture_region else {
            continue;
        };
        if !scene_texture_region_is_animated(Some(region)) {
            continue;
        }
        let region = scene_texture_region_at_elapsed(region, elapsed_ms);
        let uvs = [
            [region.u_min as f32, region.v_min as f32],
            [region.u_max as f32, region.v_min as f32],
            [region.u_min as f32, region.v_max as f32],
            [region.u_max as f32, region.v_max as f32],
        ];
        let end_index = step
            .first_index
            .checked_add(step.index_count)
            .ok_or_else(|| "scene sampled-image animated index range overflows".to_owned())?;
        let indices = geometry_indices
            .get(step.first_index as usize..end_index as usize)
            .ok_or_else(|| {
                "scene sampled-image animated index range exceeds geometry indices".to_owned()
            })?;
        let mut unique_vertices = Vec::<u32>::new();
        for index in indices {
            if !unique_vertices.contains(index) {
                unique_vertices.push(*index);
            }
        }
        if unique_vertices.len() != 4 {
            return Err(format!(
                "scene sampled-image animated draw step for layer {} expected 4 unique vertices, got {}",
                step.layer_index,
                unique_vertices.len()
            ));
        }
        let vertex_count = vertices.len();
        for (vertex_index, uv) in unique_vertices.into_iter().zip(uvs) {
            let vertex = vertices.get_mut(vertex_index as usize).ok_or_else(|| {
                format!(
                    "scene sampled-image animated vertex index {vertex_index} exceeds vertex count {}",
                    vertex_count
                )
            })?;
            vertex.uv = uv;
        }
    }
    Ok(())
}

fn scene_sampled_image_draw_steps_are_animated(
    draw_steps: &[NativeVulkanVulkanaliaSceneSampledImageDrawStep],
) -> bool {
    draw_steps
        .iter()
        .any(|step| scene_texture_region_is_animated(step.texture_region))
}

fn scene_sampled_image_vertex_buffer_count(
    draw_steps: &[NativeVulkanVulkanaliaSceneSampledImageDrawStep],
    frame_resource_count: usize,
) -> usize {
    if scene_sampled_image_draw_steps_are_animated(draw_steps) {
        frame_resource_count.max(1)
    } else {
        1
    }
}

fn scene_texture_region_is_animated(region: Option<SceneTextureRegion>) -> bool {
    region.is_some_and(|region| {
        region.frame_count > 1
            && region.columns > 0
            && region.rows > 0
            && region.fps.is_some_and(|fps| fps.is_finite() && fps > 0.0)
    })
}

fn scene_texture_region_at_elapsed(
    region: SceneTextureRegion,
    elapsed_ms: u64,
) -> SceneTextureRegion {
    let Some(fps) = region.fps.filter(|fps| fps.is_finite() && *fps > 0.0) else {
        return region;
    };
    let frame_delta = ((elapsed_ms as f64 / 1000.0) * fps).floor().max(0.0) as u64;
    let frame_count = region.frame_count.max(1);
    let frame_index = if region.loop_playback {
        ((u64::from(region.frame_index) + frame_delta) % u64::from(frame_count)) as u32
    } else {
        (u64::from(region.frame_index) + frame_delta).min(u64::from(frame_count - 1)) as u32
    };
    let columns = region.columns.max(1);
    let u_width = region.u_max - region.u_min;
    let v_height = region.v_max - region.v_min;
    let column = frame_index % columns;
    let row = frame_index / columns;
    SceneTextureRegion {
        u_min: f64::from(column) * u_width,
        v_min: f64::from(row) * v_height,
        u_max: f64::from(column + 1) * u_width,
        v_max: f64::from(row + 1) * v_height,
        frame_index,
        ..region
    }
}

fn create_scene_uploaded_buffer(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    payload: &[u8],
    usage: vk::BufferUsageFlags,
    label: &'static str,
) -> Result<VulkanaliaSceneUploadedBuffer, String> {
    if payload.is_empty() {
        return Err(format!("scene {label} buffer payload must not be empty"));
    }

    let create_info = vk::BufferCreateInfo::builder()
        .size(payload.len() as u64)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let buffer = unsafe { device.create_buffer(&create_info, None) }
        .map_err(|err| format!("vkCreateBuffer(vulkanalia scene {label}): {err:?}"))?;

    let result = (|| -> Result<VulkanaliaSceneUploadedBuffer, String> {
        let memory_requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
        let memory_type_candidates =
            native_vulkan_vulkanalia_memory_type_candidates(memory_properties);
        let memory_type = scene_buffer_memory_type_index(
            &memory_type_candidates,
            memory_requirements.memory_type_bits,
            HOST_VISIBLE_COHERENT_DEVICE_LOCAL_MEMORY_FLAG_BITS,
        )
        .or_else(|| {
            scene_buffer_memory_type_index(
                &memory_type_candidates,
                memory_requirements.memory_type_bits,
                HOST_VISIBLE_COHERENT_MEMORY_FLAG_BITS,
            )
        })
        .or_else(|| {
            scene_buffer_memory_type_index(
                &memory_type_candidates,
                memory_requirements.memory_type_bits,
                HOST_VISIBLE_MEMORY_FLAG_BITS,
            )
        })
        .ok_or_else(|| {
            format!(
                "scene {label} buffer has no host-visible memory type for bits 0x{:08x}",
                memory_requirements.memory_type_bits
            )
        })?;
        let allocation_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(memory_requirements.size)
            .memory_type_index(memory_type.index);
        let memory = unsafe { device.allocate_memory(&allocation_info, None) }
            .map_err(|err| format!("vkAllocateMemory(vulkanalia scene {label}): {err:?}"))?;

        if let Err(err) =
            native_vulkan_vulkanalia_bind_buffer_memory2(device, buffer, memory, 0, label)
        {
            unsafe {
                device.free_memory(memory, None);
            }
            return Err(err);
        }

        let map = match native_vulkan_vulkanalia_map_memory2(
            device,
            memory,
            0,
            payload.len() as u64,
            vk::MemoryMapFlags::empty(),
            label,
        ) {
            Ok(map) => map,
            Err(err) => {
                unsafe {
                    device.free_memory(memory, None);
                }
                return Err(err);
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
                let _ = native_vulkan_vulkanalia_unmap_memory2(device, memory, label);
                unsafe { device.free_memory(memory, None) };
                return Err(format!(
                    "vkFlushMappedMemoryRanges(vulkanalia scene {label}): {err:?}"
                ));
            }
        }
        native_vulkan_vulkanalia_unmap_memory2(device, memory, label)?;

        Ok(VulkanaliaSceneUploadedBuffer {
            buffer,
            memory,
            memory_type,
        })
    })();

    if result.is_err() {
        unsafe {
            device.destroy_buffer(buffer, None);
        }
    }
    result
}

fn destroy_scene_uploaded_buffer(device: &Device, buffer: VulkanaliaSceneUploadedBuffer) {
    unsafe {
        device.destroy_buffer(buffer.buffer, None);
        device.free_memory(buffer.memory, None);
    }
}

fn scene_buffer_memory_type_index(
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

fn scene_solid_quad_geometry_payload(
    input: Option<&NativeVulkanVulkanaliaSceneSolidQuadGeometryInput>,
    extent: vk::Extent2D,
    color: NativeVulkanClearColor,
    scene_size: Option<SceneSize>,
    scene_fit: FitMode,
) -> Result<VulkanaliaSceneSolidQuadGeometryPayload, String> {
    let derived_geometry;
    let transformed_geometry;
    let input = if let Some(input) = input {
        if let Some(transform) = scene_viewport_transform(scene_size, scene_fit, extent) {
            transformed_geometry = scene_solid_quad_geometry_with_viewport(input, transform);
            &transformed_geometry
        } else {
            input
        }
    } else {
        derived_geometry = scene_solid_quad_full_extent_geometry_input(extent, color);
        &derived_geometry
    };
    scene_solid_quad_geometry_payload_from_input(input)
}

fn scene_solid_quad_full_extent_geometry_input(
    extent: vk::Extent2D,
    color: NativeVulkanClearColor,
) -> NativeVulkanVulkanaliaSceneSolidQuadGeometryInput {
    let x0 = 0.0;
    let y0 = 0.0;
    let x1 = extent.width as f32;
    let y1 = extent.height as f32;
    let rgba = [color.r, color.g, color.b, color.a];
    NativeVulkanVulkanaliaSceneSolidQuadGeometryInput::new(
        vec![
            NativeVulkanVulkanaliaSceneSolidQuadVertex::new([x0, y0], rgba),
            NativeVulkanVulkanaliaSceneSolidQuadVertex::new([x1, y0], rgba),
            NativeVulkanVulkanaliaSceneSolidQuadVertex::new([x1, y1], rgba),
            NativeVulkanVulkanaliaSceneSolidQuadVertex::new([x0, y1], rgba),
        ],
        vec![0, 1, 2, 2, 3, 0],
        "full-extent-smoke-quad",
    )
}

fn scene_solid_quad_geometry_payload_from_input(
    input: &NativeVulkanVulkanaliaSceneSolidQuadGeometryInput,
) -> Result<VulkanaliaSceneSolidQuadGeometryPayload, String> {
    if input.vertices.is_empty() {
        return Err("scene solid quad geometry requires at least one vertex".to_owned());
    }
    if input.indices.is_empty() {
        return Err("scene solid quad geometry requires at least one index".to_owned());
    }
    if input.indices.len() % 3 != 0 {
        return Err("scene solid quad index payload must be a triangle list".to_owned());
    }
    if input.vertices.len() > u32::MAX as usize {
        return Err("scene solid quad vertex count exceeds u32".to_owned());
    }
    if input.indices.len() > u32::MAX as usize {
        return Err("scene solid quad index count exceeds u32".to_owned());
    }
    if input.draw_steps.is_empty() {
        return Err("scene solid quad geometry requires at least one draw step".to_owned());
    }
    for (step_index, step) in input.draw_steps.iter().enumerate() {
        if step.index_count == 0 {
            return Err(format!(
                "scene solid quad draw step {step_index} requires at least one index"
            ));
        }
        let end_index = step
            .first_index
            .checked_add(step.index_count)
            .ok_or_else(|| {
                format!("scene solid quad draw step {step_index} index range overflows")
            })?;
        if end_index as usize > input.indices.len() {
            return Err(format!(
                "scene solid quad draw step {step_index} index range {}..{} exceeds index count {}",
                step.first_index,
                end_index,
                input.indices.len()
            ));
        }
    }

    let vertex_bytes = scene_solid_quad_vertex_bytes(&input.vertices)?;
    let index_bytes = scene_solid_quad_index_bytes(&input.indices, input.vertices.len())?;
    Ok(VulkanaliaSceneSolidQuadGeometryPayload {
        indices: input.indices.clone(),
        vertex_bytes,
        index_bytes,
        vertex_count: input.vertices.len() as u32,
        index_count: input.indices.len() as u32,
        quad_count: (input.indices.len() / SCENE_FULL_SOLID_QUAD_INDEX_COUNT as usize) as u32,
        draw_steps: input.draw_steps.clone(),
        source_label: input.source_label.clone(),
    })
}

fn scene_sampled_image_geometry_payload(
    input: Option<&NativeVulkanVulkanaliaSceneSampledImageGeometryInput>,
    extent: vk::Extent2D,
    fit: Option<FitMode>,
    source_extent: vk::Extent2D,
    scene_size: Option<SceneSize>,
    scene_fit: FitMode,
) -> Result<VulkanaliaSceneSampledImageGeometryPayload, String> {
    let derived_geometry;
    let transformed_geometry;
    let input = if let Some(input) = input {
        if let Some(transform) = scene_viewport_transform(scene_size, scene_fit, extent) {
            transformed_geometry = scene_sampled_image_geometry_with_viewport(input, transform);
            &transformed_geometry
        } else {
            input
        }
    } else if let Some(fit) = fit {
        derived_geometry = scene_sampled_image_fit_geometry_input(extent, source_extent, fit)?;
        &derived_geometry
    } else {
        derived_geometry = scene_sampled_image_full_extent_geometry_input(extent);
        &derived_geometry
    };
    scene_sampled_image_geometry_payload_from_input(input)
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SceneViewportTransform {
    scale_x: f32,
    scale_y: f32,
    offset_x: f32,
    offset_y: f32,
}

fn scene_viewport_transform(
    scene_size: Option<SceneSize>,
    fit: FitMode,
    extent: vk::Extent2D,
) -> Option<SceneViewportTransform> {
    let scene_size = scene_size?;
    if scene_size.width == 0 || scene_size.height == 0 || extent.width == 0 || extent.height == 0 {
        return None;
    }
    let target_width = extent.width as f64;
    let target_height = extent.height as f64;
    let scene_width = f64::from(scene_size.width);
    let scene_height = f64::from(scene_size.height);
    let (scale_x, scale_y) = match fit {
        FitMode::Stretch => (target_width / scene_width, target_height / scene_height),
        FitMode::Contain | FitMode::Cover => {
            let scale_x = target_width / scene_width;
            let scale_y = target_height / scene_height;
            let scale = if fit == FitMode::Cover {
                scale_x.max(scale_y)
            } else {
                scale_x.min(scale_y)
            };
            (scale, scale)
        }
        FitMode::Center | FitMode::Tile => (1.0, 1.0),
    };
    let scaled_width = scene_width * scale_x;
    let scaled_height = scene_height * scale_y;
    Some(SceneViewportTransform {
        scale_x: scale_x as f32,
        scale_y: scale_y as f32,
        offset_x: ((target_width - scaled_width) * 0.5) as f32,
        offset_y: ((target_height - scaled_height) * 0.5) as f32,
    })
}

fn scene_solid_quad_geometry_with_viewport(
    input: &NativeVulkanVulkanaliaSceneSolidQuadGeometryInput,
    transform: SceneViewportTransform,
) -> NativeVulkanVulkanaliaSceneSolidQuadGeometryInput {
    let mut geometry = input.clone();
    for vertex in &mut geometry.vertices {
        vertex.position = scene_viewport_transform_position(vertex.position, transform);
    }
    geometry.source_label = format!("{}+scene-viewport-fit", geometry.source_label);
    geometry
}

fn scene_sampled_image_geometry_with_viewport(
    input: &NativeVulkanVulkanaliaSceneSampledImageGeometryInput,
    transform: SceneViewportTransform,
) -> NativeVulkanVulkanaliaSceneSampledImageGeometryInput {
    let mut geometry = input.clone();
    for vertex in &mut geometry.vertices {
        vertex.position = scene_viewport_transform_position(vertex.position, transform);
    }
    geometry.source_label = format!("{}+scene-viewport-fit", geometry.source_label);
    geometry
}

fn scene_viewport_transform_position(
    position: [f32; 2],
    transform: SceneViewportTransform,
) -> [f32; 2] {
    [
        position[0].mul_add(transform.scale_x, transform.offset_x),
        position[1].mul_add(transform.scale_y, transform.offset_y),
    ]
}

fn scene_sampled_image_full_extent_geometry_input(
    extent: vk::Extent2D,
) -> NativeVulkanVulkanaliaSceneSampledImageGeometryInput {
    let x0 = 0.0;
    let y0 = 0.0;
    let x1 = extent.width as f32;
    let y1 = extent.height as f32;
    NativeVulkanVulkanaliaSceneSampledImageGeometryInput::new(
        vec![
            NativeVulkanVulkanaliaSceneSampledImageVertex::new([x0, y0], [0.0, 0.0], 1.0),
            NativeVulkanVulkanaliaSceneSampledImageVertex::new([x1, y0], [1.0, 0.0], 1.0),
            NativeVulkanVulkanaliaSceneSampledImageVertex::new([x1, y1], [1.0, 1.0], 1.0),
            NativeVulkanVulkanaliaSceneSampledImageVertex::new([x0, y1], [0.0, 1.0], 1.0),
        ],
        vec![0, 1, 2, 2, 1, 3],
        "full-extent-smoke-sampled-image",
    )
}

fn scene_sampled_image_sources(
    source: &PathBuf,
    geometry: Option<&NativeVulkanVulkanaliaSceneSampledImageGeometryInput>,
) -> Vec<PathBuf> {
    geometry
        .and_then(|geometry| (!geometry.sources.is_empty()).then_some(geometry.sources.clone()))
        .unwrap_or_else(|| vec![source.clone()])
}

fn scene_sampled_image_load_sources(
    sources: &[PathBuf],
) -> Result<Vec<NativeVulkanVulkanaliaSceneNativeTexture>, String> {
    sources
        .iter()
        .map(|source| native_vulkan_vulkanalia_load_scene_native_texture(source))
        .collect()
}

fn create_scene_sampled_image_descriptor_heap_resources(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    plan: &super::descriptor_heap::NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    sampled_images: &[VulkanaliaSceneSampledImageResources],
) -> Result<VulkanaliaDescriptorHeapImageSamplerResources, String> {
    if sampled_images.is_empty() {
        return Err("scene descriptor heap requires at least one sampled image".to_owned());
    }
    let mut descriptor_heap =
        native_vulkan_vulkanalia_create_descriptor_heap_image_sampler_resources(
            device,
            memory_properties,
            plan,
        )?;
    descriptor_heap.snapshot.route = "scene-descriptor-heap-image-sampler-retained-resource";
    descriptor_heap.snapshot.shader_mapping_source = "heap-with-constant-offset";
    descriptor_heap.snapshot.command_order = vec![
        "create_device_addressable_resource_heap_buffer",
        "create_device_addressable_sampler_heap_buffer",
        "write_resource_descriptors_ext",
        "write_sampler_descriptors_ext",
        "cmd_bind_resource_heap_ext",
        "cmd_bind_sampler_heap_ext",
        "draw_with_descriptor_heap_constant_offset_mapping",
    ];
    for (image_index, resource) in sampled_images.iter().enumerate() {
        if let Err(err) = native_vulkan_vulkanalia_write_descriptor_heap_image_sampler(
            device,
            &mut descriptor_heap,
            image_index,
            &resource.image_view_create_info,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            &resource.sampler_create_info,
        ) {
            native_vulkan_vulkanalia_destroy_descriptor_heap_image_sampler_resources(
                device,
                descriptor_heap,
            );
            return Err(err);
        }
    }
    descriptor_heap.snapshot.zero_copy_gate = "scene sampled-image heap descriptors point at retained Vulkan images; present draws sample retained GPU images without per-frame descriptor set updates";
    Ok(descriptor_heap)
}

fn scene_sampled_image_draw_commands(
    draw_steps: &[NativeVulkanVulkanaliaSceneSampledImageDrawStep],
    sampled_images: &[VulkanaliaSceneSampledImageResources],
) -> Result<Vec<VulkanaliaSceneSampledImageDrawCommand>, String> {
    let mut draw_commands = Vec::with_capacity(draw_steps.len());
    for (step_index, step) in draw_steps.iter().enumerate() {
        sampled_images.get(step.resource_index as usize).ok_or_else(|| {
            format!(
                "scene sampled-image draw step {step_index} resource index {} exceeds sampled image count {}",
                step.resource_index,
                sampled_images.len()
            )
        })?;
        let descriptor_binding = VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap {
            resource_index: step.resource_index,
        };
        draw_commands.push(VulkanaliaSceneSampledImageDrawCommand {
            layer_index: step.layer_index,
            descriptor_binding,
            first_index: step.first_index,
            index_count: step.index_count,
        });
    }
    Ok(draw_commands)
}

fn scene_solid_quad_draw_commands(
    draw_steps: &[NativeVulkanVulkanaliaSceneSolidQuadDrawStep],
) -> Result<Vec<VulkanaliaSceneSolidQuadDrawCommand>, String> {
    if draw_steps.is_empty() {
        return Err("scene solid draw command list requires at least one step".to_owned());
    }
    let mut draw_commands = Vec::with_capacity(draw_steps.len());
    for (step_index, step) in draw_steps.iter().enumerate() {
        if step.index_count == 0 {
            return Err(format!(
                "scene solid draw step {step_index} requires at least one index"
            ));
        }
        draw_commands.push(VulkanaliaSceneSolidQuadDrawCommand {
            layer_index: step.layer_index,
            first_index: step.first_index,
            index_count: step.index_count,
        });
    }
    Ok(draw_commands)
}

fn scene_sampled_image_sampler_mode(
    fit: Option<FitMode>,
) -> NativeVulkanVulkanaliaSceneSampledImageSamplerMode {
    if fit == Some(FitMode::Tile) {
        NativeVulkanVulkanaliaSceneSampledImageSamplerMode::Repeat
    } else {
        NativeVulkanVulkanaliaSceneSampledImageSamplerMode::ClampToEdge
    }
}

fn scene_sampled_image_resource_sampler_mode(
    resource_index: usize,
    draw_steps: &[NativeVulkanVulkanaliaSceneSampledImageDrawStep],
    implicit_fit: Option<FitMode>,
) -> NativeVulkanVulkanaliaSceneSampledImageSamplerMode {
    draw_steps
        .get(resource_index)
        .and_then(|step| step.fit)
        .or(implicit_fit)
        .map_or(
            NativeVulkanVulkanaliaSceneSampledImageSamplerMode::ClampToEdge,
            |fit| scene_sampled_image_sampler_mode(Some(fit)),
        )
}

fn scene_sampled_image_fit_geometry_input(
    extent: vk::Extent2D,
    source_extent: vk::Extent2D,
    fit: FitMode,
) -> Result<NativeVulkanVulkanaliaSceneSampledImageGeometryInput, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("scene sampled-image fit geometry requires non-zero target extent".into());
    }
    if source_extent.width == 0 || source_extent.height == 0 {
        return Err("scene sampled-image fit geometry requires non-zero source extent".into());
    }

    let target_width = extent.width as f64;
    let target_height = extent.height as f64;
    let source_width = source_extent.width as f64;
    let source_height = source_extent.height as f64;
    let (scaled_width, scaled_height, uv_x1, uv_y1) = match fit {
        FitMode::Stretch => (target_width, target_height, 1.0, 1.0),
        FitMode::Center => (source_width, source_height, 1.0, 1.0),
        FitMode::Contain | FitMode::Cover => {
            let scale_x = target_width / source_width;
            let scale_y = target_height / source_height;
            let scale = if fit == FitMode::Cover {
                scale_x.max(scale_y)
            } else {
                scale_x.min(scale_y)
            };
            (
                (source_width * scale).round().max(1.0),
                (source_height * scale).round().max(1.0),
                1.0,
                1.0,
            )
        }
        FitMode::Tile => (
            target_width,
            target_height,
            target_width / source_width,
            target_height / source_height,
        ),
    };
    let x0 = ((target_width - scaled_width) * 0.5) as f32;
    let y0 = ((target_height - scaled_height) * 0.5) as f32;
    let x1 = (x0 as f64 + scaled_width) as f32;
    let y1 = (y0 as f64 + scaled_height) as f32;
    let uv_x1 = uv_x1 as f32;
    let uv_y1 = uv_y1 as f32;

    Ok(NativeVulkanVulkanaliaSceneSampledImageGeometryInput::new(
        vec![
            NativeVulkanVulkanaliaSceneSampledImageVertex::new([x0, y0], [0.0, 0.0], 1.0),
            NativeVulkanVulkanaliaSceneSampledImageVertex::new([x1, y0], [uv_x1, 0.0], 1.0),
            NativeVulkanVulkanaliaSceneSampledImageVertex::new([x0, y1], [0.0, uv_y1], 1.0),
            NativeVulkanVulkanaliaSceneSampledImageVertex::new([x1, y1], [uv_x1, uv_y1], 1.0),
        ],
        vec![0, 1, 2, 2, 1, 3],
        format!("fit-{fit:?}-sampled-image"),
    ))
}

fn scene_sampled_image_geometry_payload_from_input(
    input: &NativeVulkanVulkanaliaSceneSampledImageGeometryInput,
) -> Result<VulkanaliaSceneSampledImageGeometryPayload, String> {
    if input.vertices.is_empty() {
        return Err("scene sampled-image geometry requires at least one vertex".to_owned());
    }
    if input.indices.is_empty() {
        return Err("scene sampled-image geometry requires at least one index".to_owned());
    }
    if input.indices.len() % 3 != 0 {
        return Err("scene sampled-image index payload must be a triangle list".to_owned());
    }
    if input.vertices.len() > u32::MAX as usize {
        return Err("scene sampled-image vertex count exceeds u32".to_owned());
    }
    if input.indices.len() > u32::MAX as usize {
        return Err("scene sampled-image index count exceeds u32".to_owned());
    }
    if input.draw_steps.is_empty() {
        return Err("scene sampled-image geometry requires at least one draw step".to_owned());
    }
    if !input.sources.is_empty() && input.sources.len() != input.draw_steps.len() {
        return Err(format!(
            "scene sampled-image geometry requires source count {} to match draw step count {}",
            input.sources.len(),
            input.draw_steps.len()
        ));
    }
    let source_count = input.sources.len().max(1);
    for (step_index, step) in input.draw_steps.iter().enumerate() {
        if step.index_count == 0 {
            return Err(format!(
                "scene sampled-image draw step {step_index} requires at least one index"
            ));
        }
        if step.resource_index as usize >= source_count {
            return Err(format!(
                "scene sampled-image draw step {step_index} resource index {} exceeds source count {}",
                step.resource_index, source_count
            ));
        }
        let end_index = step
            .first_index
            .checked_add(step.index_count)
            .ok_or_else(|| {
                format!("scene sampled-image draw step {step_index} index range overflows")
            })?;
        if end_index as usize > input.indices.len() {
            return Err(format!(
                "scene sampled-image draw step {step_index} index range {}..{} exceeds index count {}",
                step.first_index,
                end_index,
                input.indices.len()
            ));
        }
    }

    let vertex_bytes = scene_sampled_image_vertex_bytes(&input.vertices)?;
    let index_bytes =
        scene_geometry_index_bytes(&input.indices, input.vertices.len(), "sampled-image")?;
    Ok(VulkanaliaSceneSampledImageGeometryPayload {
        vertices: input.vertices.clone(),
        indices: input.indices.clone(),
        sources: input.sources.clone(),
        vertex_bytes,
        index_bytes,
        vertex_count: input.vertices.len() as u32,
        index_count: input.indices.len() as u32,
        quad_count: (input.indices.len() / SCENE_FULL_SAMPLED_IMAGE_INDEX_COUNT as usize) as u32,
        source_count: source_count.min(u32::MAX as usize) as u32,
        draw_steps: input.draw_steps.clone(),
        source_label: input.source_label.clone(),
    })
}

fn scene_solid_quad_vertex_bytes(
    vertices: &[NativeVulkanVulkanaliaSceneSolidQuadVertex],
) -> Result<Vec<u8>, String> {
    let mut bytes =
        Vec::with_capacity(vertices.len() * SCENE_FULL_SOLID_QUAD_VERTEX_STRIDE_BYTES as usize);
    for (index, vertex) in vertices.iter().enumerate() {
        if !vertex
            .position
            .into_iter()
            .chain(vertex.rgba)
            .all(f32::is_finite)
        {
            return Err(format!(
                "scene solid quad vertex {index} contains a non-finite value"
            ));
        }
        for value in vertex.position.into_iter().chain(vertex.rgba) {
            bytes.extend_from_slice(&value.to_ne_bytes());
        }
    }
    Ok(bytes)
}

fn scene_sampled_image_vertex_bytes(
    vertices: &[NativeVulkanVulkanaliaSceneSampledImageVertex],
) -> Result<Vec<u8>, String> {
    let mut bytes =
        Vec::with_capacity(vertices.len() * SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES as usize);
    for (index, vertex) in vertices.iter().enumerate() {
        if !vertex
            .position
            .into_iter()
            .chain(vertex.uv)
            .chain([vertex.opacity])
            .all(f32::is_finite)
        {
            return Err(format!(
                "scene sampled-image vertex {index} contains a non-finite value"
            ));
        }
        for value in vertex
            .position
            .into_iter()
            .chain(vertex.uv)
            .chain([vertex.opacity])
        {
            bytes.extend_from_slice(&value.to_ne_bytes());
        }
    }
    Ok(bytes)
}

fn scene_solid_quad_index_bytes(indices: &[u32], vertex_count: usize) -> Result<Vec<u8>, String> {
    scene_geometry_index_bytes(indices, vertex_count, "solid quad")
}

fn scene_geometry_index_bytes(
    indices: &[u32],
    vertex_count: usize,
    label: &'static str,
) -> Result<Vec<u8>, String> {
    let max_index = (vertex_count - 1) as u32;
    let mut bytes = Vec::with_capacity(indices.len() * 4);
    for index in indices {
        if *index > max_index {
            return Err(format!(
                "scene {label} index {index} exceeds max vertex index {max_index}"
            ));
        }
        bytes.extend_from_slice(&index.to_ne_bytes());
    }
    Ok(bytes)
}

fn submit_scene_solid_quad_command_buffer2(
    device: &Device,
    queue: vk::Queue,
    command_buffer: vk::CommandBuffer,
    image_available: vk::Semaphore,
    render_finished: vk::Semaphore,
    fence: vk::Fence,
) -> Result<(), String> {
    let wait = vk::SemaphoreSubmitInfo::builder()
        .semaphore(image_available)
        .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        .build();
    let waits = [wait];
    let command_buffer_info = vk::CommandBufferSubmitInfo::builder()
        .command_buffer(command_buffer)
        .build();
    let command_buffer_infos = [command_buffer_info];
    let signal = vk::SemaphoreSubmitInfo::builder()
        .semaphore(render_finished)
        .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .build();
    let signals = [signal];
    let submit_info = vk::SubmitInfo2::builder()
        .wait_semaphore_infos(&waits)
        .command_buffer_infos(&command_buffer_infos)
        .signal_semaphore_infos(&signals)
        .build();

    unsafe {
        device
            .queue_submit2(queue, &[submit_info], fence)
            .map_err(|err| format!("vkQueueSubmit2(vulkanalia scene present): {err:?}"))?;
    }

    Ok(())
}

fn scene_color_subresource_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1)
        .build()
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
    .filter_map(|(flag, label)| {
        if flags & flag == flag {
            Some(label)
        } else {
            None
        }
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solid_quad_vertices_cover_full_pixel_extent() {
        let payload = scene_solid_quad_geometry_payload(
            None,
            vk::Extent2D {
                width: 1000,
                height: 500,
            },
            NativeVulkanClearColor {
                r: 0.25,
                g: 0.5,
                b: 0.75,
                a: 1.0,
            },
            None,
            FitMode::Cover,
        )
        .unwrap();

        assert_eq!(payload.source_label, "full-extent-smoke-quad");
        assert_eq!(payload.vertex_count, 4);
        assert_eq!(payload.index_count, 6);
        assert_eq!(payload.quad_count, 1);
        assert_eq!(payload.vertex_bytes.len(), 96);
        let floats = payload
            .vertex_bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_ne_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(&floats[0..6], &[0.0, 0.0, 0.25, 0.5, 0.75, 1.0]);
        assert_eq!(&floats[18..24], &[0.0, 500.0, 0.25, 0.5, 0.75, 1.0]);
    }

    #[test]
    fn solid_quad_indices_match_two_triangles() {
        let input = scene_solid_quad_full_extent_geometry_input(
            vk::Extent2D {
                width: 1000,
                height: 500,
            },
            NativeVulkanClearColor {
                r: 0.25,
                g: 0.5,
                b: 0.75,
                a: 1.0,
            },
        );
        let bytes = scene_solid_quad_index_bytes(&input.indices, input.vertices.len())
            .expect("full extent quad indices");
        let indices = bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_ne_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();

        assert_eq!(indices, vec![0, 1, 2, 2, 3, 0]);
    }

    #[test]
    fn solid_quad_geometry_accepts_scene_draw_plan_payload() {
        let input = NativeVulkanVulkanaliaSceneSolidQuadGeometryInput::new(
            vec![
                NativeVulkanVulkanaliaSceneSolidQuadVertex::new(
                    [-160.0, -78.0],
                    [0.2, 0.4, 0.6, 0.75],
                ),
                NativeVulkanVulkanaliaSceneSolidQuadVertex::new(
                    [160.0, -78.0],
                    [0.2, 0.4, 0.6, 0.75],
                ),
                NativeVulkanVulkanaliaSceneSolidQuadVertex::new(
                    [-160.0, 102.0],
                    [0.2, 0.4, 0.6, 0.75],
                ),
                NativeVulkanVulkanaliaSceneSolidQuadVertex::new(
                    [160.0, 102.0],
                    [0.2, 0.4, 0.6, 0.75],
                ),
            ],
            vec![0, 1, 2, 2, 1, 3],
            "scene-runtime-draw-plan",
        );

        let payload = scene_solid_quad_geometry_payload_from_input(&input).unwrap();

        assert_eq!(payload.source_label, "scene-runtime-draw-plan");
        assert_eq!(payload.vertex_count, 4);
        assert_eq!(payload.index_count, 6);
        assert_eq!(payload.quad_count, 1);
        assert_eq!(payload.vertex_bytes.len(), 96);
        assert_eq!(payload.index_bytes.len(), 24);
        let indices = payload
            .index_bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_ne_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(indices, vec![0, 1, 2, 2, 1, 3]);
    }

    #[test]
    fn solid_quad_geometry_rejects_out_of_range_indices() {
        let input = NativeVulkanVulkanaliaSceneSolidQuadGeometryInput::new(
            vec![
                NativeVulkanVulkanaliaSceneSolidQuadVertex::new([0.0, 0.0], [1.0, 0.0, 0.0, 1.0]),
                NativeVulkanVulkanaliaSceneSolidQuadVertex::new([1.0, 0.0], [1.0, 0.0, 0.0, 1.0]),
                NativeVulkanVulkanaliaSceneSolidQuadVertex::new([0.0, 1.0], [1.0, 0.0, 0.0, 1.0]),
            ],
            vec![0, 1, 3],
            "bad-geometry",
        );

        let err = scene_solid_quad_geometry_payload_from_input(&input).unwrap_err();

        assert!(err.contains("exceeds max vertex index"));
    }

    #[test]
    fn sampled_image_vertices_cover_full_pixel_extent() {
        let payload = scene_sampled_image_geometry_payload(
            None,
            vk::Extent2D {
                width: 1000,
                height: 500,
            },
            None,
            vk::Extent2D {
                width: 1000,
                height: 500,
            },
            None,
            FitMode::Cover,
        )
        .unwrap();

        assert_eq!(payload.source_label, "full-extent-smoke-sampled-image");
        assert_eq!(payload.vertex_count, 4);
        assert_eq!(payload.index_count, 6);
        assert_eq!(payload.quad_count, 1);
        assert_eq!(payload.vertex_bytes.len(), 80);
        assert_eq!(payload.index_bytes.len(), 24);
        let floats = payload
            .vertex_bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_ne_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(&floats[0..5], &[0.0, 0.0, 0.0, 0.0, 1.0]);
        assert_eq!(&floats[15..20], &[0.0, 500.0, 0.0, 1.0, 1.0]);
    }

    #[test]
    fn sampled_image_fit_geometry_matches_static_fit_semantics() {
        let target = vk::Extent2D {
            width: 1000,
            height: 500,
        };
        let source = vk::Extent2D {
            width: 400,
            height: 400,
        };

        let cover = scene_sampled_image_fit_geometry_input(target, source, FitMode::Cover)
            .expect("cover geometry");
        assert_eq!(
            sampled_image_positions(&cover),
            vec![
                [0.0, -250.0],
                [1000.0, -250.0],
                [0.0, 750.0],
                [1000.0, 750.0]
            ]
        );

        let contain = scene_sampled_image_fit_geometry_input(target, source, FitMode::Contain)
            .expect("contain geometry");
        assert_eq!(
            sampled_image_positions(&contain),
            vec![[250.0, 0.0], [750.0, 0.0], [250.0, 500.0], [750.0, 500.0]]
        );

        let stretch = scene_sampled_image_fit_geometry_input(target, source, FitMode::Stretch)
            .expect("stretch geometry");
        assert_eq!(
            sampled_image_positions(&stretch),
            vec![[0.0, 0.0], [1000.0, 0.0], [0.0, 500.0], [1000.0, 500.0]]
        );

        let center = scene_sampled_image_fit_geometry_input(target, source, FitMode::Center)
            .expect("center geometry");
        assert_eq!(
            sampled_image_positions(&center),
            vec![[300.0, 50.0], [700.0, 50.0], [300.0, 450.0], [700.0, 450.0]]
        );

        let tile = scene_sampled_image_fit_geometry_input(target, source, FitMode::Tile)
            .expect("tile geometry");
        assert_eq!(
            sampled_image_positions(&tile),
            vec![[0.0, 0.0], [1000.0, 0.0], [0.0, 500.0], [1000.0, 500.0]]
        );
        assert_eq!(
            sampled_image_uvs(&tile),
            vec![[0.0, 0.0], [2.5, 0.0], [0.0, 1.25], [2.5, 1.25]]
        );
        assert_eq!(
            scene_sampled_image_sampler_mode(Some(FitMode::Tile)),
            NativeVulkanVulkanaliaSceneSampledImageSamplerMode::Repeat
        );
        assert_eq!(
            scene_sampled_image_sampler_mode(Some(FitMode::Cover)),
            NativeVulkanVulkanaliaSceneSampledImageSamplerMode::ClampToEdge
        );
        assert_eq!(
            scene_sampled_image_sampler_mode(None),
            NativeVulkanVulkanaliaSceneSampledImageSamplerMode::ClampToEdge
        );
    }

    #[test]
    fn sampled_image_scene_viewport_cover_centers_scene_space_payload() {
        let input = NativeVulkanVulkanaliaSceneSampledImageGeometryInput::new(
            vec![
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([0.0, 0.0], [0.0, 0.0], 1.0),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([2160.0, 0.0], [1.0, 0.0], 1.0),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([0.0, 1440.0], [0.0, 1.0], 1.0),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new(
                    [2160.0, 1440.0],
                    [1.0, 1.0],
                    1.0,
                ),
            ],
            vec![0, 1, 2, 2, 1, 3],
            "scene-space-atlas",
        );

        let payload = scene_sampled_image_geometry_payload(
            Some(&input),
            vk::Extent2D {
                width: 2561,
                height: 1601,
            },
            None,
            vk::Extent2D {
                width: 6480,
                height: 5760,
            },
            Some(SceneSize {
                width: 2160,
                height: 1440,
            }),
            FitMode::Cover,
        )
        .unwrap();

        assert_eq!(payload.source_label, "scene-space-atlas+scene-viewport-fit");
        let floats = payload
            .vertex_bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_ne_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_close(floats[0], 0.0);
        assert_close(floats[1], -53.166668);
        assert_close(floats[5], 2561.0);
        assert_close(floats[6], -53.166668);
        assert_close(floats[15], 2561.0);
        assert_close(floats[16], 1654.1666);
    }

    #[test]
    fn sampled_image_texture_region_advances_with_elapsed_runtime() {
        let region = SceneTextureRegion {
            u_min: 0.0,
            v_min: 0.0,
            u_max: 1.0 / 3.0,
            v_max: 0.25,
            frame_index: 0,
            frame_count: 12,
            columns: 3,
            rows: 4,
            fps: Some(12.0),
            loop_playback: true,
        };

        let sixth = scene_texture_region_at_elapsed(region, 417);
        assert_eq!(sixth.frame_index, 5);
        assert_close_f64(sixth.u_min, 2.0 / 3.0);
        assert_close_f64(sixth.v_min, 0.25);
        assert_close_f64(sixth.u_max, 1.0);
        assert_close_f64(sixth.v_max, 0.5);

        let looped = scene_texture_region_at_elapsed(region, 1000);
        assert_eq!(looped.frame_index, 0);
        assert_close_f64(looped.u_min, 0.0);
        assert_close_f64(looped.v_min, 0.0);
    }

    #[test]
    fn sampled_image_atlas_region_applies_to_dynamic_timeline_vertices() {
        let mut vertices = vec![
            NativeVulkanVulkanaliaSceneSampledImageVertex::new([40.0, 20.0], [0.0, 0.0], 0.5),
            NativeVulkanVulkanaliaSceneSampledImageVertex::new([140.0, 20.0], [0.0, 0.0], 0.5),
            NativeVulkanVulkanaliaSceneSampledImageVertex::new([40.0, 120.0], [0.0, 0.0], 0.5),
            NativeVulkanVulkanaliaSceneSampledImageVertex::new([140.0, 120.0], [0.0, 0.0], 0.5),
        ];
        let indices = vec![0, 1, 2, 2, 1, 3];
        let draw_steps = vec![NativeVulkanVulkanaliaSceneSampledImageDrawStep {
            layer_index: 7,
            resource_index: 0,
            first_index: 0,
            index_count: 6,
            fit: Some(FitMode::Cover),
            texture_region: Some(SceneTextureRegion {
                u_min: 0.0,
                v_min: 0.0,
                u_max: 1.0 / 3.0,
                v_max: 0.25,
                frame_index: 0,
                frame_count: 12,
                columns: 3,
                rows: 4,
                fps: Some(12.0),
                loop_playback: true,
            }),
        }];

        native_vulkan_scene_apply_elapsed_texture_regions(
            &mut vertices,
            &indices,
            &draw_steps,
            417,
        )
        .unwrap();

        assert_eq!(vertices[0].position, [40.0, 20.0]);
        assert_eq!(vertices[3].position, [140.0, 120.0]);
        assert_close(vertices[0].uv[0], 2.0 / 3.0);
        assert_close(vertices[0].uv[1], 0.25);
        assert_close(vertices[3].uv[0], 1.0);
        assert_close(vertices[3].uv[1], 0.5);
        assert_close(vertices[0].opacity, 0.5);
    }

    #[test]
    fn sampled_image_vertex_buffer_count_uses_frame_slots_only_for_animated_atlas() {
        let static_steps = [NativeVulkanVulkanaliaSceneSampledImageDrawStep {
            layer_index: 0,
            resource_index: 0,
            first_index: 0,
            index_count: 6,
            fit: Some(FitMode::Cover),
            texture_region: None,
        }];
        let animated_steps = [NativeVulkanVulkanaliaSceneSampledImageDrawStep {
            texture_region: Some(SceneTextureRegion {
                u_min: 0.0,
                v_min: 0.0,
                u_max: 1.0 / 3.0,
                v_max: 0.25,
                frame_index: 0,
                frame_count: 12,
                columns: 3,
                rows: 4,
                fps: Some(12.0),
                loop_playback: true,
            }),
            ..static_steps[0]
        }];

        assert_eq!(scene_sampled_image_vertex_buffer_count(&static_steps, 3), 1);
        assert_eq!(
            scene_sampled_image_vertex_buffer_count(&animated_steps, 3),
            3
        );
        assert_eq!(
            scene_sampled_image_vertex_buffer_count(&animated_steps, 0),
            1
        );
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.001,
            "expected {actual} to be within 0.001 of {expected}"
        );
    }

    fn assert_close_f64(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 0.001,
            "expected {actual} to be within 0.001 of {expected}"
        );
    }

    fn sampled_image_positions(
        input: &NativeVulkanVulkanaliaSceneSampledImageGeometryInput,
    ) -> Vec<[f32; 2]> {
        input
            .vertices
            .iter()
            .map(|vertex| vertex.position)
            .collect()
    }

    fn sampled_image_uvs(
        input: &NativeVulkanVulkanaliaSceneSampledImageGeometryInput,
    ) -> Vec<[f32; 2]> {
        input.vertices.iter().map(|vertex| vertex.uv).collect()
    }

    #[test]
    fn sampled_image_indices_match_two_triangles() {
        let input = scene_sampled_image_full_extent_geometry_input(vk::Extent2D {
            width: 1000,
            height: 500,
        });
        let bytes = scene_geometry_index_bytes(&input.indices, input.vertices.len(), "test")
            .expect("full extent sampled-image indices");
        let indices = bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_ne_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();

        assert_eq!(indices, vec![0, 1, 2, 2, 1, 3]);
    }

    #[test]
    fn sampled_image_geometry_accepts_scene_draw_plan_payload() {
        let input = NativeVulkanVulkanaliaSceneSampledImageGeometryInput::new(
            vec![
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([-90.0, -50.0], [0.0, 0.0], 0.5),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([110.0, -50.0], [1.0, 0.0], 0.5),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([-90.0, 50.0], [0.0, 1.0], 0.5),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([110.0, 50.0], [1.0, 1.0], 0.5),
            ],
            vec![0, 1, 2, 2, 1, 3],
            "scene-runtime-sampled-image-draw-plan",
        );

        let payload = scene_sampled_image_geometry_payload_from_input(&input).unwrap();

        assert_eq!(
            payload.source_label,
            "scene-runtime-sampled-image-draw-plan"
        );
        assert_eq!(payload.vertex_count, 4);
        assert_eq!(payload.index_count, 6);
        assert_eq!(payload.quad_count, 1);
        assert_eq!(payload.vertex_bytes.len(), 80);
        assert_eq!(payload.index_bytes.len(), 24);
        let indices = payload
            .index_bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_ne_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(indices, vec![0, 1, 2, 2, 1, 3]);
    }

    #[test]
    fn sampled_image_geometry_accepts_batched_scene_payload() {
        let input = NativeVulkanVulkanaliaSceneSampledImageGeometryInput::new_batched(
            vec![
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([0.0, 0.0], [0.0, 0.0], 1.0),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([10.0, 0.0], [1.0, 0.0], 1.0),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([0.0, 10.0], [0.0, 1.0], 1.0),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([10.0, 10.0], [1.0, 1.0], 1.0),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([20.0, 20.0], [0.0, 0.0], 0.5),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([30.0, 20.0], [2.0, 0.0], 0.5),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([20.0, 30.0], [0.0, 2.0], 0.5),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([30.0, 30.0], [2.0, 2.0], 0.5),
            ],
            vec![0, 1, 2, 2, 1, 3, 4, 5, 6, 6, 5, 7],
            vec![PathBuf::from("/tmp/a.png"), PathBuf::from("/tmp/b.png")],
            vec![
                NativeVulkanVulkanaliaSceneSampledImageDrawStep {
                    layer_index: 0,
                    resource_index: 0,
                    first_index: 0,
                    index_count: 6,
                    fit: Some(FitMode::Cover),
                    texture_region: None,
                },
                NativeVulkanVulkanaliaSceneSampledImageDrawStep {
                    layer_index: 1,
                    resource_index: 1,
                    first_index: 6,
                    index_count: 6,
                    fit: Some(FitMode::Tile),
                    texture_region: None,
                },
            ],
            "batched-scene-runtime-sampled-image-draw-plan",
        );

        let payload = scene_sampled_image_geometry_payload_from_input(&input).unwrap();

        assert_eq!(payload.vertex_count, 8);
        assert_eq!(payload.index_count, 12);
        assert_eq!(payload.quad_count, 2);
        assert_eq!(payload.source_count, 2);
        assert_eq!(payload.draw_steps.len(), 2);
        assert_eq!(payload.draw_steps[1].resource_index, 1);
        assert_eq!(payload.draw_steps[1].first_index, 6);
        assert_eq!(payload.draw_steps[1].index_count, 6);
        assert_eq!(
            scene_sampled_image_resource_sampler_mode(1, &payload.draw_steps, None),
            NativeVulkanVulkanaliaSceneSampledImageSamplerMode::Repeat
        );
    }

    #[test]
    fn sampled_image_geometry_rejects_out_of_range_indices() {
        let input = NativeVulkanVulkanaliaSceneSampledImageGeometryInput::new(
            vec![
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([0.0, 0.0], [0.0, 0.0], 1.0),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([1.0, 0.0], [1.0, 0.0], 1.0),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([0.0, 1.0], [0.0, 1.0], 1.0),
            ],
            vec![0, 1, 3],
            "bad-sampled-image-geometry",
        );

        let err = scene_sampled_image_geometry_payload_from_input(&input).unwrap_err();

        assert!(err.contains("scene sampled-image index 3 exceeds max vertex index 2"));
    }

    #[test]
    fn memory_type_selection_prefers_host_visible_coherent_device_local() {
        let memory_types = vec![
            NativeVulkanVulkanaliaMemoryTypeCandidate {
                index: 0,
                property_flags_bits: vk::MemoryPropertyFlags::HOST_VISIBLE.bits(),
            },
            NativeVulkanVulkanaliaMemoryTypeCandidate {
                index: 1,
                property_flags_bits: vk::MemoryPropertyFlags::HOST_VISIBLE.bits()
                    | vk::MemoryPropertyFlags::HOST_COHERENT.bits()
                    | vk::MemoryPropertyFlags::DEVICE_LOCAL.bits(),
            },
        ];

        let selected = scene_buffer_memory_type_index(
            &memory_types,
            0b11,
            HOST_VISIBLE_COHERENT_DEVICE_LOCAL_MEMORY_FLAG_BITS,
        )
        .unwrap();

        assert_eq!(selected.index, 1);
    }
}
