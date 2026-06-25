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

use crate::core::FitMode;
use crate::renderer::native_vulkan::NativeVulkanClearColor;
use crate::renderer::native_wayland::{
    NativeWaylandHost, NativeWaylandHostOptions, NativeWaylandSurfaceHandles,
};

use super::instance::{
    NativeVulkanVulkanaliaInstance,
    native_vulkan_vulkanalia_create_instance_with_required_extensions,
    native_vulkan_vulkanalia_destroy_instance,
};
use super::present_timing::VulkanaliaPresentTimingConfig;
use super::scene_lite_draw_pass::{
    NativeVulkanVulkanaliaSceneLiteSampledImageCommandSnapshot,
    NativeVulkanVulkanaliaSceneLiteSampledImagePipelineSnapshot,
    NativeVulkanVulkanaliaSceneLiteSolidQuadCommandSnapshot,
    NativeVulkanVulkanaliaSceneLiteSolidQuadPipelineSnapshot,
    VulkanaliaSceneLiteSampledImageDescriptorBinding, VulkanaliaSceneLiteSampledImageDrawCommand,
    VulkanaliaSceneLiteSampledImagePipelineResources, VulkanaliaSceneLiteSolidQuadDrawResources,
    VulkanaliaSceneLiteSolidQuadPipelineResources,
    native_vulkan_vulkanalia_create_scene_lite_sampled_image_pipeline_resources,
    native_vulkan_vulkanalia_create_scene_lite_solid_quad_pipeline_resources,
    native_vulkan_vulkanalia_destroy_scene_lite_sampled_image_pipeline_resources,
    native_vulkan_vulkanalia_destroy_scene_lite_solid_quad_pipeline_resources,
    native_vulkan_vulkanalia_record_scene_lite_sampled_image_command_buffer,
    native_vulkan_vulkanalia_record_scene_lite_solid_quad_command_buffer,
};
use super::scene_lite_sampled_image::{
    NativeVulkanVulkanaliaSceneLiteDecodedRgbaImage,
    NativeVulkanVulkanaliaSceneLiteSampledImageDescriptorStrategySnapshot,
    NativeVulkanVulkanaliaSceneLiteSampledImageResourceSnapshot,
    NativeVulkanVulkanaliaSceneLiteSampledImageSamplerMode,
    VulkanaliaSceneLiteSampledImageResources,
    native_vulkan_vulkanalia_create_scene_lite_sampled_image_resources,
    native_vulkan_vulkanalia_decode_scene_lite_rgba_image,
    native_vulkan_vulkanalia_destroy_scene_lite_sampled_image_resources,
    native_vulkan_vulkanalia_scene_lite_sampled_image_descriptor_strategy,
};
use super::swapchain::{
    NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot,
    NativeVulkanVulkanaliaPresentQueueSnapshot, NativeVulkanVulkanaliaSwapchainSnapshot,
    OPTIONAL_INSTANCE_EXTENSIONS, REQUIRED_INSTANCE_EXTENSIONS, composite_alpha_label,
    create_vulkanalia_present_device, create_vulkanalia_swapchain_plan,
    create_vulkanalia_wayland_surface, present_mode_label, queue_flag_labels,
    select_vulkanalia_present_queue, swapchain_create_flag_labels,
    vulkanalia_surface_capabilities2_enabled,
};
use super::video_session::{
    NativeVulkanVulkanaliaMemoryTypeCandidate, native_vulkan_vulkanalia_memory_type_candidates,
};

const SCENE_LITE_SOLID_QUAD_INDEX_COUNT: u32 = 6;
const SCENE_LITE_SOLID_QUAD_VERTEX_STRIDE_BYTES: u32 = 24;
const SCENE_LITE_SAMPLED_IMAGE_INDEX_COUNT: u32 = 6;
const SCENE_LITE_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES: u32 = 20;
const HOST_VISIBLE_COHERENT_MEMORY_FLAG_BITS: u32 =
    vk::MemoryPropertyFlags::HOST_VISIBLE.bits() | vk::MemoryPropertyFlags::HOST_COHERENT.bits();
const HOST_VISIBLE_COHERENT_DEVICE_LOCAL_MEMORY_FLAG_BITS: u32 =
    HOST_VISIBLE_COHERENT_MEMORY_FLAG_BITS | vk::MemoryPropertyFlags::DEVICE_LOCAL.bits();
const HOST_VISIBLE_MEMORY_FLAG_BITS: u32 = vk::MemoryPropertyFlags::HOST_VISIBLE.bits();

#[derive(Debug, Clone, PartialEq)]
pub struct NativeVulkanVulkanaliaSceneLiteSolidQuadPresentOptions {
    pub host: NativeWaylandHostOptions,
    pub wait_configure_roundtrips: usize,
    pub duration: Duration,
    pub target_max_fps: Option<u32>,
    pub quad_color: NativeVulkanClearColor,
    pub geometry: Option<NativeVulkanVulkanaliaSceneLiteSolidQuadGeometryInput>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NativeVulkanVulkanaliaSceneLiteSampledImagePresentOptions {
    pub host: NativeWaylandHostOptions,
    pub wait_configure_roundtrips: usize,
    pub duration: Duration,
    pub target_max_fps: Option<u32>,
    pub source: PathBuf,
    pub clear_color: NativeVulkanClearColor,
    pub fit: Option<FitMode>,
    pub solid_geometry: Option<NativeVulkanVulkanaliaSceneLiteSolidQuadGeometryInput>,
    pub geometry: Option<NativeVulkanVulkanaliaSceneLiteSampledImageGeometryInput>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeVulkanVulkanaliaSceneLiteSolidQuadVertex {
    pub position: [f32; 2],
    pub rgba: [f32; 4],
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeVulkanVulkanaliaSceneLiteSampledImageVertex {
    pub position: [f32; 2],
    pub uv: [f32; 2],
    pub opacity: f32,
}

impl NativeVulkanVulkanaliaSceneLiteSampledImageVertex {
    pub fn new(position: [f32; 2], uv: [f32; 2], opacity: f32) -> Self {
        Self {
            position,
            uv,
            opacity,
        }
    }
}

impl NativeVulkanVulkanaliaSceneLiteSolidQuadVertex {
    pub fn new(position: [f32; 2], rgba: [f32; 4]) -> Self {
        Self { position, rgba }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NativeVulkanVulkanaliaSceneLiteSolidQuadGeometryInput {
    pub vertices: Vec<NativeVulkanVulkanaliaSceneLiteSolidQuadVertex>,
    pub indices: Vec<u32>,
    pub source_label: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NativeVulkanVulkanaliaSceneLiteSampledImageGeometryInput {
    pub vertices: Vec<NativeVulkanVulkanaliaSceneLiteSampledImageVertex>,
    pub indices: Vec<u32>,
    pub sources: Vec<PathBuf>,
    pub draw_steps: Vec<NativeVulkanVulkanaliaSceneLiteSampledImageDrawStep>,
    pub source_label: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeVulkanVulkanaliaSceneLiteSampledImageDrawStep {
    pub resource_index: u32,
    pub first_index: u32,
    pub index_count: u32,
    pub fit: Option<FitMode>,
}

impl NativeVulkanVulkanaliaSceneLiteSampledImageGeometryInput {
    pub fn new(
        vertices: Vec<NativeVulkanVulkanaliaSceneLiteSampledImageVertex>,
        indices: Vec<u32>,
        source_label: impl Into<String>,
    ) -> Self {
        let index_count = indices.len().min(u32::MAX as usize) as u32;
        Self {
            vertices,
            indices,
            sources: Vec::new(),
            draw_steps: vec![NativeVulkanVulkanaliaSceneLiteSampledImageDrawStep {
                resource_index: 0,
                first_index: 0,
                index_count,
                fit: None,
            }],
            source_label: source_label.into(),
        }
    }

    pub fn new_batched(
        vertices: Vec<NativeVulkanVulkanaliaSceneLiteSampledImageVertex>,
        indices: Vec<u32>,
        sources: Vec<PathBuf>,
        draw_steps: Vec<NativeVulkanVulkanaliaSceneLiteSampledImageDrawStep>,
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

impl NativeVulkanVulkanaliaSceneLiteSolidQuadGeometryInput {
    pub fn new(
        vertices: Vec<NativeVulkanVulkanaliaSceneLiteSolidQuadVertex>,
        indices: Vec<u32>,
        source_label: impl Into<String>,
    ) -> Self {
        Self {
            vertices,
            indices,
            source_label: source_label.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanVulkanaliaSceneLiteSolidQuadPresentSnapshot {
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
    pub geometry: NativeVulkanVulkanaliaSceneLiteSolidQuadGeometrySnapshot,
    pub pipeline: NativeVulkanVulkanaliaSceneLiteSolidQuadPipelineSnapshot,
    pub last_command: Option<NativeVulkanVulkanaliaSceneLiteSolidQuadCommandSnapshot>,
    pub command_submit_model: &'static str,
    pub present_sync_model: &'static str,
    pub wait_idle_after_present: bool,
    pub present_ids: Vec<Option<u64>>,
    pub uses_present_id: bool,
    pub uses_present_id2: bool,
    pub present_wait_available: bool,
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
pub struct NativeVulkanVulkanaliaSceneLiteSampledImagePresentSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
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
    pub solid_geometry: Option<NativeVulkanVulkanaliaSceneLiteSolidQuadGeometrySnapshot>,
    pub solid_pipeline: Option<NativeVulkanVulkanaliaSceneLiteSolidQuadPipelineSnapshot>,
    pub geometry: NativeVulkanVulkanaliaSceneLiteSampledImageGeometrySnapshot,
    pub sampled_image: NativeVulkanVulkanaliaSceneLiteSampledImageResourceSnapshot,
    pub sampled_images: Vec<NativeVulkanVulkanaliaSceneLiteSampledImageResourceSnapshot>,
    pub descriptor_strategy: NativeVulkanVulkanaliaSceneLiteSampledImageDescriptorStrategySnapshot,
    pub pipeline: NativeVulkanVulkanaliaSceneLiteSampledImagePipelineSnapshot,
    pub last_command: Option<NativeVulkanVulkanaliaSceneLiteSampledImageCommandSnapshot>,
    pub command_submit_model: &'static str,
    pub present_sync_model: &'static str,
    pub wait_idle_after_present: bool,
    pub present_ids: Vec<Option<u64>>,
    pub uses_present_id: bool,
    pub uses_present_id2: bool,
    pub present_wait_available: bool,
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
pub struct NativeVulkanVulkanaliaSceneLiteSolidQuadGeometrySnapshot {
    pub source_label: String,
    pub vertex_count: u32,
    pub vertex_buffer_bytes: u64,
    pub index_buffer_bytes: u64,
    pub index_count: u32,
    pub quad_count: u32,
    pub vertex_stride_bytes: u32,
    pub selected_vertex_memory_type_index: u32,
    pub selected_index_memory_type_index: u32,
    pub vertex_memory_property_flags: Vec<&'static str>,
    pub index_memory_property_flags: Vec<&'static str>,
    pub upload_model: &'static str,
    pub retained_across_frames: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSceneLiteSampledImageGeometrySnapshot {
    pub source_label: String,
    pub vertex_count: u32,
    pub vertex_buffer_bytes: u64,
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

struct VulkanaliaSceneLiteSolidQuadGeometryResources {
    vertex_buffer: vk::Buffer,
    vertex_memory: vk::DeviceMemory,
    index_buffer: vk::Buffer,
    index_memory: vk::DeviceMemory,
    snapshot: NativeVulkanVulkanaliaSceneLiteSolidQuadGeometrySnapshot,
}

struct VulkanaliaSceneLiteSampledImageGeometryResources {
    vertex_buffer: vk::Buffer,
    vertex_memory: vk::DeviceMemory,
    index_buffer: vk::Buffer,
    index_memory: vk::DeviceMemory,
    draw_steps: Vec<NativeVulkanVulkanaliaSceneLiteSampledImageDrawStep>,
    snapshot: NativeVulkanVulkanaliaSceneLiteSampledImageGeometrySnapshot,
}

struct VulkanaliaSceneLiteSolidQuadFrameResources {
    swapchain_image_views: Vec<vk::ImageView>,
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    image_available: Vec<vk::Semaphore>,
    render_finished: Vec<vk::Semaphore>,
    in_flight: Vec<vk::Fence>,
}

struct VulkanaliaSceneLiteUploadedBuffer {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    memory_type: NativeVulkanVulkanaliaMemoryTypeCandidate,
}

#[derive(Debug)]
struct VulkanaliaSceneLiteSolidQuadGeometryPayload {
    vertex_bytes: Vec<u8>,
    index_bytes: Vec<u8>,
    vertex_count: u32,
    index_count: u32,
    quad_count: u32,
    source_label: String,
}

#[derive(Debug)]
struct VulkanaliaSceneLiteSampledImageGeometryPayload {
    vertex_bytes: Vec<u8>,
    index_bytes: Vec<u8>,
    vertex_count: u32,
    index_count: u32,
    quad_count: u32,
    source_count: u32,
    draw_steps: Vec<NativeVulkanVulkanaliaSceneLiteSampledImageDrawStep>,
    source_label: String,
}

pub fn run_native_vulkan_vulkanalia_scene_lite_solid_quad_present(
    options: NativeVulkanVulkanaliaSceneLiteSolidQuadPresentOptions,
) -> Result<NativeVulkanVulkanaliaSceneLiteSolidQuadPresentSnapshot, String> {
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
    let result = run_vulkanalia_scene_lite_solid_quad_present_inner(&vulkan, handles, options);
    native_vulkan_vulkanalia_destroy_instance(vulkan);
    result
}

fn run_vulkanalia_scene_lite_solid_quad_present_inner(
    vulkan: &NativeVulkanVulkanaliaInstance,
    handles: NativeWaylandSurfaceHandles,
    options: NativeVulkanVulkanaliaSceneLiteSolidQuadPresentOptions,
) -> Result<NativeVulkanVulkanaliaSceneLiteSolidQuadPresentSnapshot, String> {
    let instance = &vulkan.instance;
    let surface = create_vulkanalia_wayland_surface(instance, handles)?;
    let result =
        with_vulkanalia_scene_lite_solid_quad_present(instance, surface, handles, vulkan, options);
    unsafe {
        instance.destroy_surface_khr(surface, None);
    }
    result
}

pub fn run_native_vulkan_vulkanalia_scene_lite_sampled_image_present(
    options: NativeVulkanVulkanaliaSceneLiteSampledImagePresentOptions,
) -> Result<NativeVulkanVulkanaliaSceneLiteSampledImagePresentSnapshot, String> {
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
    let result = run_vulkanalia_scene_lite_sampled_image_present_inner(&vulkan, handles, options);
    native_vulkan_vulkanalia_destroy_instance(vulkan);
    result
}

fn run_vulkanalia_scene_lite_sampled_image_present_inner(
    vulkan: &NativeVulkanVulkanaliaInstance,
    handles: NativeWaylandSurfaceHandles,
    options: NativeVulkanVulkanaliaSceneLiteSampledImagePresentOptions,
) -> Result<NativeVulkanVulkanaliaSceneLiteSampledImagePresentSnapshot, String> {
    let instance = &vulkan.instance;
    let surface = create_vulkanalia_wayland_surface(instance, handles)?;
    let result = with_vulkanalia_scene_lite_sampled_image_present(
        instance, surface, handles, vulkan, options,
    );
    unsafe {
        instance.destroy_surface_khr(surface, None);
    }
    result
}

fn with_vulkanalia_scene_lite_solid_quad_present(
    instance: &Instance,
    surface: vk::SurfaceKHR,
    handles: NativeWaylandSurfaceHandles,
    vulkan: &NativeVulkanVulkanaliaInstance,
    options: NativeVulkanVulkanaliaSceneLiteSolidQuadPresentOptions,
) -> Result<NativeVulkanVulkanaliaSceneLiteSolidQuadPresentSnapshot, String> {
    let physical_devices = unsafe { instance.enumerate_physical_devices() }.map_err(|err| {
        format!("vkEnumeratePhysicalDevices(vulkanalia scene-lite present): {err:?}")
    })?;
    let mut present_queue_family_count = 0usize;
    let selection = select_vulkanalia_present_queue(
        instance,
        surface,
        handles,
        &physical_devices,
        &mut present_queue_family_count,
    )?;
    let present_device = create_vulkanalia_present_device(instance, &selection)?;
    if !present_device.feature_selection.synchronization2_enabled {
        unsafe {
            present_device.device.destroy_device(None);
        }
        return Err(
            "Vulkanalia scene-lite present requires synchronization2 for QueueSubmit2".to_owned(),
        );
    }
    if !present_device.feature_selection.dynamic_rendering_enabled {
        unsafe {
            present_device.device.destroy_device(None);
        }
        return Err(
            "Vulkanalia scene-lite present requires dynamicRendering for CmdBeginRendering"
                .to_owned(),
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
                "vkCreateSwapchainKHR(vulkanalia scene-lite present): {err:?}"
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
                "vkGetSwapchainImagesKHR(vulkanalia scene-lite present): {err:?}"
            ));
        }
    };

    let frame_resources = match create_scene_lite_solid_quad_frame_resources(
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
    let pipeline = match native_vulkan_vulkanalia_create_scene_lite_solid_quad_pipeline_resources(
        device,
        swapchain_plan.format.format,
        swapchain_plan.extent,
    ) {
        Ok(pipeline) => pipeline,
        Err(err) => {
            destroy_scene_lite_solid_quad_frame_resources(device, frame_resources);
            unsafe {
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(err);
        }
    };
    let memory_properties =
        unsafe { instance.get_physical_device_memory_properties(selection.physical_device) };
    let geometry_payload = match scene_lite_solid_quad_geometry_payload(
        options.geometry.as_ref(),
        swapchain_plan.extent,
        options.quad_color,
    ) {
        Ok(payload) => payload,
        Err(err) => {
            native_vulkan_vulkanalia_destroy_scene_lite_solid_quad_pipeline_resources(
                device, pipeline,
            );
            destroy_scene_lite_solid_quad_frame_resources(device, frame_resources);
            unsafe {
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(err);
        }
    };
    let geometry = match create_scene_lite_solid_quad_geometry_resources(
        device,
        &memory_properties,
        geometry_payload,
    ) {
        Ok(geometry) => geometry,
        Err(err) => {
            native_vulkan_vulkanalia_destroy_scene_lite_solid_quad_pipeline_resources(
                device, pipeline,
            );
            destroy_scene_lite_solid_quad_frame_resources(device, frame_resources);
            unsafe {
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(err);
        }
    };
    let present_timing = VulkanaliaPresentTimingConfig::new(
        present_device.feature_selection.present_id_enabled,
        swapchain_plan.present_id2_enabled,
        present_device.feature_selection.present_wait_enabled,
        swapchain_plan.present_wait2_enabled,
    );

    let result = run_scene_lite_solid_quad_present_loop(
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
    destroy_scene_lite_solid_quad_geometry_resources(device, geometry);
    native_vulkan_vulkanalia_destroy_scene_lite_solid_quad_pipeline_resources(device, pipeline);
    destroy_scene_lite_solid_quad_frame_resources(device, frame_resources);
    unsafe {
        device.destroy_swapchain_khr(swapchain, None);
        present_device.device.destroy_device(None);
    }

    result
}

fn with_vulkanalia_scene_lite_sampled_image_present(
    instance: &Instance,
    surface: vk::SurfaceKHR,
    handles: NativeWaylandSurfaceHandles,
    vulkan: &NativeVulkanVulkanaliaInstance,
    options: NativeVulkanVulkanaliaSceneLiteSampledImagePresentOptions,
) -> Result<NativeVulkanVulkanaliaSceneLiteSampledImagePresentSnapshot, String> {
    let physical_devices = unsafe { instance.enumerate_physical_devices() }.map_err(|err| {
        format!("vkEnumeratePhysicalDevices(vulkanalia scene-lite sampled image present): {err:?}")
    })?;
    let mut present_queue_family_count = 0usize;
    let selection = select_vulkanalia_present_queue(
        instance,
        surface,
        handles,
        &physical_devices,
        &mut present_queue_family_count,
    )?;
    let present_device = create_vulkanalia_present_device(instance, &selection)?;
    if !present_device.feature_selection.synchronization2_enabled {
        unsafe {
            present_device.device.destroy_device(None);
        }
        return Err("Vulkanalia scene-lite sampled image present requires synchronization2 for QueueSubmit2".to_owned());
    }
    if !present_device.feature_selection.dynamic_rendering_enabled {
        unsafe {
            present_device.device.destroy_device(None);
        }
        return Err("Vulkanalia scene-lite sampled image present requires dynamicRendering for CmdBeginRendering".to_owned());
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
                "vkCreateSwapchainKHR(vulkanalia scene-lite sampled image present): {err:?}"
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
                "vkGetSwapchainImagesKHR(vulkanalia scene-lite sampled image present): {err:?}"
            ));
        }
    };

    let frame_resources = match create_scene_lite_solid_quad_frame_resources(
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
        scene_lite_sampled_image_sources(&options.source, options.geometry.as_ref());
    let descriptor_strategy = native_vulkan_vulkanalia_scene_lite_sampled_image_descriptor_strategy(
        present_device.feature_selection.core_features,
        present_device.feature_selection.vulkan_1_4_properties,
        sampled_image_sources.len(),
    );
    let pipeline = match native_vulkan_vulkanalia_create_scene_lite_sampled_image_pipeline_resources(
        device,
        swapchain_plan.format.format,
        swapchain_plan.extent,
        descriptor_strategy.uses_push_descriptor_fast_path,
    ) {
        Ok(pipeline) => pipeline,
        Err(err) => {
            destroy_scene_lite_solid_quad_frame_resources(device, frame_resources);
            unsafe {
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(err);
        }
    };
    let memory_properties =
        unsafe { instance.get_physical_device_memory_properties(selection.physical_device) };
    let decoded_images = match scene_lite_sampled_image_decode_sources(&sampled_image_sources) {
        Ok(decoded_images) => decoded_images,
        Err(err) => {
            native_vulkan_vulkanalia_destroy_scene_lite_sampled_image_pipeline_resources(
                device, pipeline,
            );
            destroy_scene_lite_solid_quad_frame_resources(device, frame_resources);
            unsafe {
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(err);
        }
    };
    let source_extent = decoded_images
        .first()
        .map(|decoded| vk::Extent2D {
            width: decoded.width,
            height: decoded.height,
        })
        .unwrap_or(vk::Extent2D {
            width: 0,
            height: 0,
        });
    let geometry_payload = match scene_lite_sampled_image_geometry_payload(
        options.geometry.as_ref(),
        swapchain_plan.extent,
        options.fit,
        source_extent,
    ) {
        Ok(payload) => payload,
        Err(err) => {
            native_vulkan_vulkanalia_destroy_scene_lite_sampled_image_pipeline_resources(
                device, pipeline,
            );
            destroy_scene_lite_solid_quad_frame_resources(device, frame_resources);
            unsafe {
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(err);
        }
    };
    let geometry = match create_scene_lite_sampled_image_geometry_resources(
        device,
        &memory_properties,
        geometry_payload,
    ) {
        Ok(geometry) => geometry,
        Err(err) => {
            native_vulkan_vulkanalia_destroy_scene_lite_sampled_image_pipeline_resources(
                device, pipeline,
            );
            destroy_scene_lite_solid_quad_frame_resources(device, frame_resources);
            unsafe {
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(err);
        }
    };
    let mut sampled_images = Vec::new();
    for (resource_index, decoded) in decoded_images.iter().enumerate() {
        let resource = match native_vulkan_vulkanalia_create_scene_lite_sampled_image_resources(
            device,
            &memory_properties,
            frame_resources.command_pool,
            present_device.queue,
            pipeline.descriptor_set_layout,
            descriptor_strategy.uses_push_descriptor_fast_path,
            scene_lite_sampled_image_resource_sampler_mode(
                resource_index,
                &geometry.draw_steps,
                options.fit,
            ),
            decoded.source.display().to_string(),
            vk::Extent2D {
                width: decoded.width,
                height: decoded.height,
            },
            &decoded.bytes,
        ) {
            Ok(resources) => resources,
            Err(err) => {
                for resource in sampled_images.drain(..) {
                    native_vulkan_vulkanalia_destroy_scene_lite_sampled_image_resources(
                        device, resource,
                    );
                }
                destroy_scene_lite_sampled_image_geometry_resources(device, geometry);
                native_vulkan_vulkanalia_destroy_scene_lite_sampled_image_pipeline_resources(
                    device, pipeline,
                );
                destroy_scene_lite_solid_quad_frame_resources(device, frame_resources);
                unsafe {
                    device.destroy_swapchain_khr(swapchain, None);
                    present_device.device.destroy_device(None);
                }
                return Err(err);
            }
        };
        sampled_images.push(resource);
    }
    let draw_commands = match scene_lite_sampled_image_draw_commands(
        &geometry.draw_steps,
        &sampled_images,
        descriptor_strategy.uses_push_descriptor_fast_path,
    ) {
        Ok(draw_commands) => draw_commands,
        Err(err) => {
            for resource in sampled_images.drain(..) {
                native_vulkan_vulkanalia_destroy_scene_lite_sampled_image_resources(
                    device, resource,
                );
            }
            destroy_scene_lite_sampled_image_geometry_resources(device, geometry);
            native_vulkan_vulkanalia_destroy_scene_lite_sampled_image_pipeline_resources(
                device, pipeline,
            );
            destroy_scene_lite_solid_quad_frame_resources(device, frame_resources);
            unsafe {
                device.destroy_swapchain_khr(swapchain, None);
                present_device.device.destroy_device(None);
            }
            return Err(err);
        }
    };
    let solid_pipeline = if options.solid_geometry.is_some() {
        match native_vulkan_vulkanalia_create_scene_lite_solid_quad_pipeline_resources(
            device,
            swapchain_plan.format.format,
            swapchain_plan.extent,
        ) {
            Ok(pipeline) => Some(pipeline),
            Err(err) => {
                for resource in sampled_images.drain(..) {
                    native_vulkan_vulkanalia_destroy_scene_lite_sampled_image_resources(
                        device, resource,
                    );
                }
                destroy_scene_lite_sampled_image_geometry_resources(device, geometry);
                native_vulkan_vulkanalia_destroy_scene_lite_sampled_image_pipeline_resources(
                    device, pipeline,
                );
                destroy_scene_lite_solid_quad_frame_resources(device, frame_resources);
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
        let payload = match scene_lite_solid_quad_geometry_payload(
            Some(solid_geometry_input),
            swapchain_plan.extent,
            options.clear_color,
        ) {
            Ok(payload) => payload,
            Err(err) => {
                if let Some(solid_pipeline) = solid_pipeline {
                    native_vulkan_vulkanalia_destroy_scene_lite_solid_quad_pipeline_resources(
                        device,
                        solid_pipeline,
                    );
                }
                for resource in sampled_images.drain(..) {
                    native_vulkan_vulkanalia_destroy_scene_lite_sampled_image_resources(
                        device, resource,
                    );
                }
                destroy_scene_lite_sampled_image_geometry_resources(device, geometry);
                native_vulkan_vulkanalia_destroy_scene_lite_sampled_image_pipeline_resources(
                    device, pipeline,
                );
                destroy_scene_lite_solid_quad_frame_resources(device, frame_resources);
                unsafe {
                    device.destroy_swapchain_khr(swapchain, None);
                    present_device.device.destroy_device(None);
                }
                return Err(err);
            }
        };
        match create_scene_lite_solid_quad_geometry_resources(device, &memory_properties, payload) {
            Ok(geometry) => Some(geometry),
            Err(err) => {
                if let Some(solid_pipeline) = solid_pipeline {
                    native_vulkan_vulkanalia_destroy_scene_lite_solid_quad_pipeline_resources(
                        device,
                        solid_pipeline,
                    );
                }
                for resource in sampled_images.drain(..) {
                    native_vulkan_vulkanalia_destroy_scene_lite_sampled_image_resources(
                        device, resource,
                    );
                }
                destroy_scene_lite_sampled_image_geometry_resources(device, geometry);
                native_vulkan_vulkanalia_destroy_scene_lite_sampled_image_pipeline_resources(
                    device, pipeline,
                );
                destroy_scene_lite_solid_quad_frame_resources(device, frame_resources);
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
        present_device.feature_selection.present_id_enabled,
        swapchain_plan.present_id2_enabled,
        present_device.feature_selection.present_wait_enabled,
        swapchain_plan.present_wait2_enabled,
    );

    let result = run_scene_lite_sampled_image_present_loop(
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
        descriptor_strategy,
        &selection,
        &present_device.extension_snapshot,
        &swapchain_plan,
        present_timing,
        options,
    );

    let _ = unsafe { device.device_wait_idle() };
    if let Some(solid_geometry) = solid_geometry {
        destroy_scene_lite_solid_quad_geometry_resources(device, solid_geometry);
    }
    if let Some(solid_pipeline) = solid_pipeline {
        native_vulkan_vulkanalia_destroy_scene_lite_solid_quad_pipeline_resources(
            device,
            solid_pipeline,
        );
    }
    for resource in sampled_images {
        native_vulkan_vulkanalia_destroy_scene_lite_sampled_image_resources(device, resource);
    }
    destroy_scene_lite_sampled_image_geometry_resources(device, geometry);
    native_vulkan_vulkanalia_destroy_scene_lite_sampled_image_pipeline_resources(device, pipeline);
    destroy_scene_lite_solid_quad_frame_resources(device, frame_resources);
    unsafe {
        device.destroy_swapchain_khr(swapchain, None);
        present_device.device.destroy_device(None);
    }

    result
}

#[allow(clippy::too_many_arguments)]
fn run_scene_lite_solid_quad_present_loop(
    vulkan: &NativeVulkanVulkanaliaInstance,
    device: &Device,
    queue: vk::Queue,
    swapchain: vk::SwapchainKHR,
    swapchain_images: &[vk::Image],
    extent: vk::Extent2D,
    frame_resources: &VulkanaliaSceneLiteSolidQuadFrameResources,
    pipeline: &VulkanaliaSceneLiteSolidQuadPipelineResources,
    geometry: &VulkanaliaSceneLiteSolidQuadGeometryResources,
    selection: &super::swapchain::NativeVulkanVulkanaliaPresentQueueSelection,
    extension_snapshot: &NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot,
    swapchain_plan: &super::swapchain::NativeVulkanVulkanaliaSwapchainPlan,
    present_timing: VulkanaliaPresentTimingConfig,
    options: NativeVulkanVulkanaliaSceneLiteSolidQuadPresentOptions,
) -> Result<NativeVulkanVulkanaliaSceneLiteSolidQuadPresentSnapshot, String> {
    let started_at = Instant::now();
    let deadline = started_at + options.duration;
    let frame_interval = options
        .target_max_fps
        .filter(|fps| *fps > 0)
        .map(|fps| Duration::from_secs_f64(1.0 / fps as f64));
    let mut next_frame = Instant::now();
    let mut frames_presented = 0u64;
    let mut present_ids = Vec::new();
    let mut last_command = None;

    while Instant::now() < deadline {
        let present_frame_slot = frames_presented as usize % frame_resources.in_flight.len();
        let image_available = frame_resources.image_available[present_frame_slot];
        let render_finished = frame_resources.render_finished[present_frame_slot];
        let in_flight = frame_resources.in_flight[present_frame_slot];
        unsafe {
            device
                .wait_for_fences(&[in_flight], true, u64::MAX)
                .map_err(|err| {
                    format!("vkWaitForFences(vulkanalia scene-lite present): {err:?}")
                })?;
            device
                .reset_fences(&[in_flight])
                .map_err(|err| format!("vkResetFences(vulkanalia scene-lite present): {err:?}"))?;
        }

        let (image_index, _) = unsafe {
            device.acquire_next_image_khr(swapchain, u64::MAX, image_available, vk::Fence::null())
        }
        .map_err(|err| format!("vkAcquireNextImageKHR(vulkanalia scene-lite present): {err:?}"))?;
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

        let command = native_vulkan_vulkanalia_record_scene_lite_solid_quad_command_buffer(
            device,
            command_buffer,
            swapchain_image,
            swapchain_view,
            extent,
            pipeline,
            geometry.vertex_buffer,
            geometry.index_buffer,
            geometry.snapshot.index_count,
        )?;
        submit_scene_lite_solid_quad_command_buffer2(
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
        let mut present_id_info = present_id.map(|_| {
            vk::PresentIdKHR::builder()
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
        } else if present_timing.present_id_enabled {
            if let Some(present_id_info) = present_id_info.as_mut() {
                present_info = present_info.push_next(present_id_info);
            }
        }
        unsafe {
            device
                .queue_present_khr(queue, &present_info)
                .map_err(|err| {
                    format!("vkQueuePresentKHR(vulkanalia scene-lite present): {err:?}")
                })?;
        }

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
    Ok(NativeVulkanVulkanaliaSceneLiteSolidQuadPresentSnapshot {
        binding: "vulkanalia",
        route: "scene-lite-solid-quad-visible-present",
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
        command_submit_model: "acquire_next_image_khr -> cmd_begin_rendering solid quad -> queue_submit2 -> queue_present_khr",
        present_sync_model: "frame-slot semaphore/fence reuse; no per-present queue_wait_idle",
        wait_idle_after_present: false,
        present_ids,
        uses_present_id: present_timing.present_id_enabled,
        uses_present_id2: present_timing.present_id2_enabled,
        present_wait_available: present_timing.present_wait_enabled,
        present_wait2_available: present_timing.present_wait2_enabled,
        present_wait_after_present: false,
        uses_pipeline_rendering_create_info: true,
        uses_dynamic_rendering: true,
        uses_synchronization2: true,
        uses_submit2: true,
        zero_copy_scope: "scene geometry is retained in Vulkan buffers and rendered directly to the swapchain",
        primary_reference: "Vulkan dynamic rendering; FFmpeg remains first reference for clock/queue discipline",
    })
}

#[allow(clippy::too_many_arguments)]
fn run_scene_lite_sampled_image_present_loop(
    vulkan: &NativeVulkanVulkanaliaInstance,
    device: &Device,
    queue: vk::Queue,
    swapchain: vk::SwapchainKHR,
    swapchain_images: &[vk::Image],
    extent: vk::Extent2D,
    frame_resources: &VulkanaliaSceneLiteSolidQuadFrameResources,
    pipeline: &VulkanaliaSceneLiteSampledImagePipelineResources,
    geometry: &VulkanaliaSceneLiteSampledImageGeometryResources,
    solid_pipeline: Option<&VulkanaliaSceneLiteSolidQuadPipelineResources>,
    solid_geometry: Option<&VulkanaliaSceneLiteSolidQuadGeometryResources>,
    sampled_images: &[VulkanaliaSceneLiteSampledImageResources],
    draw_commands: &[VulkanaliaSceneLiteSampledImageDrawCommand],
    descriptor_strategy: NativeVulkanVulkanaliaSceneLiteSampledImageDescriptorStrategySnapshot,
    selection: &super::swapchain::NativeVulkanVulkanaliaPresentQueueSelection,
    extension_snapshot: &NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot,
    swapchain_plan: &super::swapchain::NativeVulkanVulkanaliaSwapchainPlan,
    present_timing: VulkanaliaPresentTimingConfig,
    options: NativeVulkanVulkanaliaSceneLiteSampledImagePresentOptions,
) -> Result<NativeVulkanVulkanaliaSceneLiteSampledImagePresentSnapshot, String> {
    let started_at = Instant::now();
    let deadline = started_at + options.duration;
    let frame_interval = options
        .target_max_fps
        .filter(|fps| *fps > 0)
        .map(|fps| Duration::from_secs_f64(1.0 / fps as f64));
    let mut next_frame = Instant::now();
    let mut frames_presented = 0u64;
    let mut present_ids = Vec::new();
    let mut last_command = None;
    let sampled_image = sampled_images.first().ok_or_else(|| {
        "scene-lite sampled image present requires at least one sampled image".to_owned()
    })?;
    let solid_quad_draw = match (solid_pipeline, solid_geometry) {
        (Some(pipeline_resources), Some(geometry)) => {
            Some(VulkanaliaSceneLiteSolidQuadDrawResources {
                pipeline_resources,
                vertex_buffer: geometry.vertex_buffer,
                index_buffer: geometry.index_buffer,
                index_count: geometry.snapshot.index_count,
            })
        }
        (None, None) => None,
        _ => {
            return Err(
                "scene-lite mixed present requires both solid pipeline and solid geometry"
                    .to_owned(),
            );
        }
    };

    while Instant::now() < deadline {
        let present_frame_slot = frames_presented as usize % frame_resources.in_flight.len();
        let image_available = frame_resources.image_available[present_frame_slot];
        let render_finished = frame_resources.render_finished[present_frame_slot];
        let in_flight = frame_resources.in_flight[present_frame_slot];
        unsafe {
            device
                .wait_for_fences(&[in_flight], true, u64::MAX)
                .map_err(|err| {
                    format!("vkWaitForFences(vulkanalia scene-lite sampled image present): {err:?}")
                })?;
            device.reset_fences(&[in_flight]).map_err(|err| {
                format!("vkResetFences(vulkanalia scene-lite sampled image present): {err:?}")
            })?;
        }

        let (image_index, _) = unsafe {
            device.acquire_next_image_khr(swapchain, u64::MAX, image_available, vk::Fence::null())
        }
        .map_err(|err| {
            format!("vkAcquireNextImageKHR(vulkanalia scene-lite sampled image present): {err:?}")
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

        let command = native_vulkan_vulkanalia_record_scene_lite_sampled_image_command_buffer(
            device,
            command_buffer,
            swapchain_image,
            swapchain_view,
            extent,
            solid_quad_draw,
            pipeline,
            draw_commands,
            geometry.vertex_buffer,
            geometry.index_buffer,
            [
                options.clear_color.r,
                options.clear_color.g,
                options.clear_color.b,
                options.clear_color.a,
            ],
        )?;
        submit_scene_lite_solid_quad_command_buffer2(
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
        let mut present_id_info = present_id.map(|_| {
            vk::PresentIdKHR::builder()
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
        } else if present_timing.present_id_enabled {
            if let Some(present_id_info) = present_id_info.as_mut() {
                present_info = present_info.push_next(present_id_info);
            }
        }
        unsafe {
            device
                .queue_present_khr(queue, &present_info)
                .map_err(|err| {
                    format!(
                        "vkQueuePresentKHR(vulkanalia scene-lite sampled image present): {err:?}"
                    )
                })?;
        }

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
    Ok(NativeVulkanVulkanaliaSceneLiteSampledImagePresentSnapshot {
        binding: "vulkanalia",
        route: "scene-lite-sampled-image-visible-present",
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
        pipeline: pipeline.snapshot.clone(),
        last_command,
        command_submit_model: if solid_quad_draw.is_some() {
            "acquire_next_image_khr -> cmd_begin_rendering solid quads then sampled image quads -> queue_submit2 -> queue_present_khr"
        } else {
            "acquire_next_image_khr -> cmd_begin_rendering sampled image quad -> queue_submit2 -> queue_present_khr"
        },
        present_sync_model: "frame-slot semaphore/fence reuse; no per-present queue_wait_idle",
        wait_idle_after_present: false,
        present_ids,
        uses_present_id: present_timing.present_id_enabled,
        uses_present_id2: present_timing.present_id2_enabled,
        present_wait_available: present_timing.present_wait_enabled,
        present_wait2_available: present_timing.present_wait2_enabled,
        present_wait_after_present: false,
        uses_pipeline_rendering_create_info: true,
        uses_dynamic_rendering: true,
        uses_synchronization2: true,
        uses_submit2: true,
        zero_copy_scope: if solid_quad_draw.is_some() {
            "scene geometry buffers and retained sampled images render directly to the swapchain; no fallback scene snapshot upload"
        } else {
            "source image is uploaded once into a retained Vulkan sampled image and rendered directly to the swapchain"
        },
        primary_reference: "Vulkan dynamic rendering and sync2; FFmpeg frame lifetime discipline for retained resources",
    })
}

fn create_scene_lite_solid_quad_frame_resources(
    device: &Device,
    swapchain_images: &[vk::Image],
    swapchain_format: vk::Format,
    queue_family_index: u32,
) -> Result<VulkanaliaSceneLiteSolidQuadFrameResources, String> {
    if swapchain_images.is_empty() {
        return Err("scene-lite present requires at least one swapchain image".to_owned());
    }

    let mut swapchain_image_views = Vec::new();
    let mut command_pool = vk::CommandPool::null();
    let mut image_available = Vec::new();
    let mut render_finished = Vec::new();
    let mut in_flight = Vec::new();

    let result = (|| -> Result<VulkanaliaSceneLiteSolidQuadFrameResources, String> {
        swapchain_image_views = create_scene_lite_solid_quad_swapchain_image_views(
            device,
            swapchain_images,
            swapchain_format,
        )?;

        let command_pool_info = vk::CommandPoolCreateInfo::builder()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(queue_family_index);
        command_pool =
            unsafe { device.create_command_pool(&command_pool_info, None) }.map_err(|err| {
                format!("vkCreateCommandPool(vulkanalia scene-lite present): {err:?}")
            })?;
        let command_buffer_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(swapchain_images.len() as u32);
        let command_buffers = unsafe { device.allocate_command_buffers(&command_buffer_info) }
            .map_err(|err| {
                format!("vkAllocateCommandBuffers(vulkanalia scene-lite present): {err:?}")
            })?;

        let semaphore_info = vk::SemaphoreCreateInfo::builder();
        let fence_info = vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);
        for frame_slot in 0..swapchain_images.len() {
            image_available.push(
                unsafe { device.create_semaphore(&semaphore_info, None) }.map_err(|err| {
                    format!(
                        "vkCreateSemaphore(image_available slot {frame_slot} vulkanalia scene-lite present): {err:?}"
                    )
                })?,
            );
            render_finished.push(
                unsafe { device.create_semaphore(&semaphore_info, None) }.map_err(|err| {
                    format!(
                        "vkCreateSemaphore(render_finished slot {frame_slot} vulkanalia scene-lite present): {err:?}"
                    )
                })?,
            );
            in_flight.push(
                unsafe { device.create_fence(&fence_info, None) }.map_err(|err| {
                    format!(
                        "vkCreateFence(slot {frame_slot} vulkanalia scene-lite present): {err:?}"
                    )
                })?,
            );
        }

        Ok(VulkanaliaSceneLiteSolidQuadFrameResources {
            swapchain_image_views: std::mem::take(&mut swapchain_image_views),
            command_pool,
            command_buffers,
            image_available: std::mem::take(&mut image_available),
            render_finished: std::mem::take(&mut render_finished),
            in_flight: std::mem::take(&mut in_flight),
        })
    })();

    if result.is_err() {
        destroy_partial_scene_lite_solid_quad_frame_resources(
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

fn create_scene_lite_solid_quad_swapchain_image_views(
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
            .subresource_range(scene_lite_color_subresource_range());
        match unsafe { device.create_image_view(&create_info, None) } {
            Ok(view) => views.push(view),
            Err(err) => {
                for view in views {
                    unsafe {
                        device.destroy_image_view(view, None);
                    }
                }
                return Err(format!(
                    "vkCreateImageView(vulkanalia scene-lite present swapchain): {err:?}"
                ));
            }
        }
    }
    Ok(views)
}

fn destroy_scene_lite_solid_quad_frame_resources(
    device: &Device,
    resources: VulkanaliaSceneLiteSolidQuadFrameResources,
) {
    destroy_partial_scene_lite_solid_quad_frame_resources(
        device,
        resources.swapchain_image_views,
        resources.command_pool,
        resources.image_available,
        resources.render_finished,
        resources.in_flight,
    );
}

fn destroy_partial_scene_lite_solid_quad_frame_resources(
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

fn create_scene_lite_solid_quad_geometry_resources(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    payload: VulkanaliaSceneLiteSolidQuadGeometryPayload,
) -> Result<VulkanaliaSceneLiteSolidQuadGeometryResources, String> {
    let vertex = create_scene_lite_uploaded_buffer(
        device,
        memory_properties,
        &payload.vertex_bytes,
        vk::BufferUsageFlags::VERTEX_BUFFER,
        "vertex",
    )?;
    let index = match create_scene_lite_uploaded_buffer(
        device,
        memory_properties,
        &payload.index_bytes,
        vk::BufferUsageFlags::INDEX_BUFFER,
        "index",
    ) {
        Ok(index) => index,
        Err(err) => {
            destroy_scene_lite_uploaded_buffer(device, vertex);
            return Err(err);
        }
    };

    Ok(VulkanaliaSceneLiteSolidQuadGeometryResources {
        vertex_buffer: vertex.buffer,
        vertex_memory: vertex.memory,
        index_buffer: index.buffer,
        index_memory: index.memory,
        snapshot: NativeVulkanVulkanaliaSceneLiteSolidQuadGeometrySnapshot {
            source_label: payload.source_label,
            vertex_count: payload.vertex_count,
            vertex_buffer_bytes: payload.vertex_bytes.len() as u64,
            index_buffer_bytes: payload.index_bytes.len() as u64,
            index_count: payload.index_count,
            quad_count: payload.quad_count,
            vertex_stride_bytes: SCENE_LITE_SOLID_QUAD_VERTEX_STRIDE_BYTES,
            selected_vertex_memory_type_index: vertex.memory_type.index,
            selected_index_memory_type_index: index.memory_type.index,
            vertex_memory_property_flags: memory_property_flag_labels(
                vertex.memory_type.property_flags_bits,
            ),
            index_memory_property_flags: memory_property_flag_labels(
                index.memory_type.property_flags_bits,
            ),
            upload_model: "one-time host-visible geometry upload retained across present frames",
            retained_across_frames: true,
        },
    })
}

fn destroy_scene_lite_solid_quad_geometry_resources(
    device: &Device,
    resources: VulkanaliaSceneLiteSolidQuadGeometryResources,
) {
    unsafe {
        device.destroy_buffer(resources.index_buffer, None);
        device.free_memory(resources.index_memory, None);
        device.destroy_buffer(resources.vertex_buffer, None);
        device.free_memory(resources.vertex_memory, None);
    }
}

fn create_scene_lite_sampled_image_geometry_resources(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    payload: VulkanaliaSceneLiteSampledImageGeometryPayload,
) -> Result<VulkanaliaSceneLiteSampledImageGeometryResources, String> {
    let vertex = create_scene_lite_uploaded_buffer(
        device,
        memory_properties,
        &payload.vertex_bytes,
        vk::BufferUsageFlags::VERTEX_BUFFER,
        "sampled-image vertex",
    )?;
    let index = match create_scene_lite_uploaded_buffer(
        device,
        memory_properties,
        &payload.index_bytes,
        vk::BufferUsageFlags::INDEX_BUFFER,
        "sampled-image index",
    ) {
        Ok(index) => index,
        Err(err) => {
            destroy_scene_lite_uploaded_buffer(device, vertex);
            return Err(err);
        }
    };

    Ok(VulkanaliaSceneLiteSampledImageGeometryResources {
        vertex_buffer: vertex.buffer,
        vertex_memory: vertex.memory,
        index_buffer: index.buffer,
        index_memory: index.memory,
        draw_steps: payload.draw_steps.clone(),
        snapshot: NativeVulkanVulkanaliaSceneLiteSampledImageGeometrySnapshot {
            source_label: payload.source_label,
            vertex_count: payload.vertex_count,
            vertex_buffer_bytes: payload.vertex_bytes.len() as u64,
            index_buffer_bytes: payload.index_bytes.len() as u64,
            index_count: payload.index_count,
            quad_count: payload.quad_count,
            source_count: payload.source_count,
            draw_step_count: payload.draw_steps.len().min(u32::MAX as usize) as u32,
            vertex_stride_bytes: SCENE_LITE_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES,
            selected_vertex_memory_type_index: vertex.memory_type.index,
            selected_index_memory_type_index: index.memory_type.index,
            vertex_memory_property_flags: memory_property_flag_labels(
                vertex.memory_type.property_flags_bits,
            ),
            index_memory_property_flags: memory_property_flag_labels(
                index.memory_type.property_flags_bits,
            ),
            upload_model: "one-time host-visible sampled-image geometry upload retained across present frames",
            retained_across_frames: true,
        },
    })
}

fn destroy_scene_lite_sampled_image_geometry_resources(
    device: &Device,
    resources: VulkanaliaSceneLiteSampledImageGeometryResources,
) {
    unsafe {
        device.destroy_buffer(resources.index_buffer, None);
        device.free_memory(resources.index_memory, None);
        device.destroy_buffer(resources.vertex_buffer, None);
        device.free_memory(resources.vertex_memory, None);
    }
}

fn create_scene_lite_uploaded_buffer(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    payload: &[u8],
    usage: vk::BufferUsageFlags,
    label: &'static str,
) -> Result<VulkanaliaSceneLiteUploadedBuffer, String> {
    if payload.is_empty() {
        return Err(format!(
            "scene-lite {label} buffer payload must not be empty"
        ));
    }

    let create_info = vk::BufferCreateInfo::builder()
        .size(payload.len() as u64)
        .usage(usage)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);
    let buffer = unsafe { device.create_buffer(&create_info, None) }
        .map_err(|err| format!("vkCreateBuffer(vulkanalia scene-lite {label}): {err:?}"))?;

    let result = (|| -> Result<VulkanaliaSceneLiteUploadedBuffer, String> {
        let memory_requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
        let memory_type_candidates =
            native_vulkan_vulkanalia_memory_type_candidates(memory_properties);
        let memory_type = scene_lite_buffer_memory_type_index(
            &memory_type_candidates,
            memory_requirements.memory_type_bits,
            HOST_VISIBLE_COHERENT_DEVICE_LOCAL_MEMORY_FLAG_BITS,
        )
        .or_else(|| {
            scene_lite_buffer_memory_type_index(
                &memory_type_candidates,
                memory_requirements.memory_type_bits,
                HOST_VISIBLE_COHERENT_MEMORY_FLAG_BITS,
            )
        })
        .or_else(|| {
            scene_lite_buffer_memory_type_index(
                &memory_type_candidates,
                memory_requirements.memory_type_bits,
                HOST_VISIBLE_MEMORY_FLAG_BITS,
            )
        })
        .ok_or_else(|| {
            format!(
                "scene-lite {label} buffer has no host-visible memory type for bits 0x{:08x}",
                memory_requirements.memory_type_bits
            )
        })?;
        let allocation_info = vk::MemoryAllocateInfo::builder()
            .allocation_size(memory_requirements.size)
            .memory_type_index(memory_type.index);
        let memory = unsafe { device.allocate_memory(&allocation_info, None) }
            .map_err(|err| format!("vkAllocateMemory(vulkanalia scene-lite {label}): {err:?}"))?;

        if let Err(err) = unsafe { device.bind_buffer_memory(buffer, memory, 0) } {
            unsafe {
                device.free_memory(memory, None);
            }
            return Err(format!(
                "vkBindBufferMemory(vulkanalia scene-lite {label}): {err:?}"
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
                    "vkMapMemory(vulkanalia scene-lite {label}): {err:?}"
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
                    "vkFlushMappedMemoryRanges(vulkanalia scene-lite {label}): {err:?}"
                ));
            }
        }
        unsafe {
            device.unmap_memory(memory);
        }

        Ok(VulkanaliaSceneLiteUploadedBuffer {
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

fn destroy_scene_lite_uploaded_buffer(device: &Device, buffer: VulkanaliaSceneLiteUploadedBuffer) {
    unsafe {
        device.destroy_buffer(buffer.buffer, None);
        device.free_memory(buffer.memory, None);
    }
}

fn scene_lite_buffer_memory_type_index(
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

fn scene_lite_solid_quad_geometry_payload(
    input: Option<&NativeVulkanVulkanaliaSceneLiteSolidQuadGeometryInput>,
    extent: vk::Extent2D,
    color: NativeVulkanClearColor,
) -> Result<VulkanaliaSceneLiteSolidQuadGeometryPayload, String> {
    let fallback;
    let input = if let Some(input) = input {
        input
    } else {
        fallback = scene_lite_solid_quad_full_extent_geometry_input(extent, color);
        &fallback
    };
    scene_lite_solid_quad_geometry_payload_from_input(input)
}

fn scene_lite_solid_quad_full_extent_geometry_input(
    extent: vk::Extent2D,
    color: NativeVulkanClearColor,
) -> NativeVulkanVulkanaliaSceneLiteSolidQuadGeometryInput {
    let x0 = 0.0;
    let y0 = 0.0;
    let x1 = extent.width as f32;
    let y1 = extent.height as f32;
    let rgba = [color.r, color.g, color.b, color.a];
    NativeVulkanVulkanaliaSceneLiteSolidQuadGeometryInput::new(
        vec![
            NativeVulkanVulkanaliaSceneLiteSolidQuadVertex::new([x0, y0], rgba),
            NativeVulkanVulkanaliaSceneLiteSolidQuadVertex::new([x1, y0], rgba),
            NativeVulkanVulkanaliaSceneLiteSolidQuadVertex::new([x1, y1], rgba),
            NativeVulkanVulkanaliaSceneLiteSolidQuadVertex::new([x0, y1], rgba),
        ],
        vec![0, 1, 2, 2, 3, 0],
        "full-extent-smoke-quad",
    )
}

fn scene_lite_solid_quad_geometry_payload_from_input(
    input: &NativeVulkanVulkanaliaSceneLiteSolidQuadGeometryInput,
) -> Result<VulkanaliaSceneLiteSolidQuadGeometryPayload, String> {
    if input.vertices.is_empty() {
        return Err("scene-lite solid quad geometry requires at least one vertex".to_owned());
    }
    if input.indices.is_empty() {
        return Err("scene-lite solid quad geometry requires at least one index".to_owned());
    }
    if input.indices.len() % 3 != 0 {
        return Err("scene-lite solid quad index payload must be a triangle list".to_owned());
    }
    if input.vertices.len() > u32::MAX as usize {
        return Err("scene-lite solid quad vertex count exceeds u32".to_owned());
    }
    if input.indices.len() > u32::MAX as usize {
        return Err("scene-lite solid quad index count exceeds u32".to_owned());
    }

    let vertex_bytes = scene_lite_solid_quad_vertex_bytes(&input.vertices)?;
    let index_bytes = scene_lite_solid_quad_index_bytes(&input.indices, input.vertices.len())?;
    Ok(VulkanaliaSceneLiteSolidQuadGeometryPayload {
        vertex_bytes,
        index_bytes,
        vertex_count: input.vertices.len() as u32,
        index_count: input.indices.len() as u32,
        quad_count: (input.indices.len() / SCENE_LITE_SOLID_QUAD_INDEX_COUNT as usize) as u32,
        source_label: input.source_label.clone(),
    })
}

fn scene_lite_sampled_image_geometry_payload(
    input: Option<&NativeVulkanVulkanaliaSceneLiteSampledImageGeometryInput>,
    extent: vk::Extent2D,
    fit: Option<FitMode>,
    source_extent: vk::Extent2D,
) -> Result<VulkanaliaSceneLiteSampledImageGeometryPayload, String> {
    let fallback;
    let input = if let Some(input) = input {
        input
    } else if let Some(fit) = fit {
        fallback = scene_lite_sampled_image_fit_geometry_input(extent, source_extent, fit)?;
        &fallback
    } else {
        fallback = scene_lite_sampled_image_full_extent_geometry_input(extent);
        &fallback
    };
    scene_lite_sampled_image_geometry_payload_from_input(input)
}

fn scene_lite_sampled_image_full_extent_geometry_input(
    extent: vk::Extent2D,
) -> NativeVulkanVulkanaliaSceneLiteSampledImageGeometryInput {
    let x0 = 0.0;
    let y0 = 0.0;
    let x1 = extent.width as f32;
    let y1 = extent.height as f32;
    NativeVulkanVulkanaliaSceneLiteSampledImageGeometryInput::new(
        vec![
            NativeVulkanVulkanaliaSceneLiteSampledImageVertex::new([x0, y0], [0.0, 0.0], 1.0),
            NativeVulkanVulkanaliaSceneLiteSampledImageVertex::new([x1, y0], [1.0, 0.0], 1.0),
            NativeVulkanVulkanaliaSceneLiteSampledImageVertex::new([x1, y1], [1.0, 1.0], 1.0),
            NativeVulkanVulkanaliaSceneLiteSampledImageVertex::new([x0, y1], [0.0, 1.0], 1.0),
        ],
        vec![0, 1, 2, 2, 1, 3],
        "full-extent-smoke-sampled-image",
    )
}

fn scene_lite_sampled_image_sources(
    source: &PathBuf,
    geometry: Option<&NativeVulkanVulkanaliaSceneLiteSampledImageGeometryInput>,
) -> Vec<PathBuf> {
    geometry
        .and_then(|geometry| (!geometry.sources.is_empty()).then_some(geometry.sources.clone()))
        .unwrap_or_else(|| vec![source.clone()])
}

fn scene_lite_sampled_image_decode_sources(
    sources: &[PathBuf],
) -> Result<Vec<NativeVulkanVulkanaliaSceneLiteDecodedRgbaImage>, String> {
    sources
        .iter()
        .map(|source| native_vulkan_vulkanalia_decode_scene_lite_rgba_image(source))
        .collect()
}

fn scene_lite_sampled_image_draw_commands(
    draw_steps: &[NativeVulkanVulkanaliaSceneLiteSampledImageDrawStep],
    sampled_images: &[VulkanaliaSceneLiteSampledImageResources],
    use_push_descriptor_fast_path: bool,
) -> Result<Vec<VulkanaliaSceneLiteSampledImageDrawCommand>, String> {
    let mut draw_commands = Vec::with_capacity(draw_steps.len());
    for (step_index, step) in draw_steps.iter().enumerate() {
        let resource = sampled_images.get(step.resource_index as usize).ok_or_else(|| {
            format!(
                "scene-lite sampled-image draw step {step_index} resource index {} exceeds sampled image count {}",
                step.resource_index,
                sampled_images.len()
            )
        })?;
        let descriptor_binding = if use_push_descriptor_fast_path {
            VulkanaliaSceneLiteSampledImageDescriptorBinding::PushDescriptor {
                sampler: resource.sampler,
                image_view: resource.image_view,
            }
        } else {
            VulkanaliaSceneLiteSampledImageDescriptorBinding::DescriptorSet(resource.descriptor_set)
        };
        draw_commands.push(VulkanaliaSceneLiteSampledImageDrawCommand {
            descriptor_binding,
            first_index: step.first_index,
            index_count: step.index_count,
        });
    }
    Ok(draw_commands)
}

fn scene_lite_sampled_image_sampler_mode(
    fit: Option<FitMode>,
) -> NativeVulkanVulkanaliaSceneLiteSampledImageSamplerMode {
    if fit == Some(FitMode::Tile) {
        NativeVulkanVulkanaliaSceneLiteSampledImageSamplerMode::Repeat
    } else {
        NativeVulkanVulkanaliaSceneLiteSampledImageSamplerMode::ClampToEdge
    }
}

fn scene_lite_sampled_image_resource_sampler_mode(
    resource_index: usize,
    draw_steps: &[NativeVulkanVulkanaliaSceneLiteSampledImageDrawStep],
    fallback_fit: Option<FitMode>,
) -> NativeVulkanVulkanaliaSceneLiteSampledImageSamplerMode {
    draw_steps
        .get(resource_index)
        .and_then(|step| step.fit)
        .or(fallback_fit)
        .map_or(
            NativeVulkanVulkanaliaSceneLiteSampledImageSamplerMode::ClampToEdge,
            |fit| scene_lite_sampled_image_sampler_mode(Some(fit)),
        )
}

fn scene_lite_sampled_image_fit_geometry_input(
    extent: vk::Extent2D,
    source_extent: vk::Extent2D,
    fit: FitMode,
) -> Result<NativeVulkanVulkanaliaSceneLiteSampledImageGeometryInput, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("scene-lite sampled-image fit geometry requires non-zero target extent".into());
    }
    if source_extent.width == 0 || source_extent.height == 0 {
        return Err("scene-lite sampled-image fit geometry requires non-zero source extent".into());
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

    Ok(
        NativeVulkanVulkanaliaSceneLiteSampledImageGeometryInput::new(
            vec![
                NativeVulkanVulkanaliaSceneLiteSampledImageVertex::new([x0, y0], [0.0, 0.0], 1.0),
                NativeVulkanVulkanaliaSceneLiteSampledImageVertex::new([x1, y0], [uv_x1, 0.0], 1.0),
                NativeVulkanVulkanaliaSceneLiteSampledImageVertex::new([x0, y1], [0.0, uv_y1], 1.0),
                NativeVulkanVulkanaliaSceneLiteSampledImageVertex::new(
                    [x1, y1],
                    [uv_x1, uv_y1],
                    1.0,
                ),
            ],
            vec![0, 1, 2, 2, 1, 3],
            format!("fit-{fit:?}-sampled-image"),
        ),
    )
}

fn scene_lite_sampled_image_geometry_payload_from_input(
    input: &NativeVulkanVulkanaliaSceneLiteSampledImageGeometryInput,
) -> Result<VulkanaliaSceneLiteSampledImageGeometryPayload, String> {
    if input.vertices.is_empty() {
        return Err("scene-lite sampled-image geometry requires at least one vertex".to_owned());
    }
    if input.indices.is_empty() {
        return Err("scene-lite sampled-image geometry requires at least one index".to_owned());
    }
    if input.indices.len() % 3 != 0 {
        return Err("scene-lite sampled-image index payload must be a triangle list".to_owned());
    }
    if input.vertices.len() > u32::MAX as usize {
        return Err("scene-lite sampled-image vertex count exceeds u32".to_owned());
    }
    if input.indices.len() > u32::MAX as usize {
        return Err("scene-lite sampled-image index count exceeds u32".to_owned());
    }
    if input.draw_steps.is_empty() {
        return Err("scene-lite sampled-image geometry requires at least one draw step".to_owned());
    }
    if !input.sources.is_empty() && input.sources.len() != input.draw_steps.len() {
        return Err(format!(
            "scene-lite sampled-image geometry requires source count {} to match draw step count {}",
            input.sources.len(),
            input.draw_steps.len()
        ));
    }
    let source_count = input.sources.len().max(1);
    for (step_index, step) in input.draw_steps.iter().enumerate() {
        if step.index_count == 0 {
            return Err(format!(
                "scene-lite sampled-image draw step {step_index} requires at least one index"
            ));
        }
        if step.resource_index as usize >= source_count {
            return Err(format!(
                "scene-lite sampled-image draw step {step_index} resource index {} exceeds source count {}",
                step.resource_index, source_count
            ));
        }
        let end_index = step
            .first_index
            .checked_add(step.index_count)
            .ok_or_else(|| {
                format!("scene-lite sampled-image draw step {step_index} index range overflows")
            })?;
        if end_index as usize > input.indices.len() {
            return Err(format!(
                "scene-lite sampled-image draw step {step_index} index range {}..{} exceeds index count {}",
                step.first_index,
                end_index,
                input.indices.len()
            ));
        }
    }

    let vertex_bytes = scene_lite_sampled_image_vertex_bytes(&input.vertices)?;
    let index_bytes =
        scene_lite_geometry_index_bytes(&input.indices, input.vertices.len(), "sampled-image")?;
    Ok(VulkanaliaSceneLiteSampledImageGeometryPayload {
        vertex_bytes,
        index_bytes,
        vertex_count: input.vertices.len() as u32,
        index_count: input.indices.len() as u32,
        quad_count: (input.indices.len() / SCENE_LITE_SAMPLED_IMAGE_INDEX_COUNT as usize) as u32,
        source_count: source_count.min(u32::MAX as usize) as u32,
        draw_steps: input.draw_steps.clone(),
        source_label: input.source_label.clone(),
    })
}

fn scene_lite_solid_quad_vertex_bytes(
    vertices: &[NativeVulkanVulkanaliaSceneLiteSolidQuadVertex],
) -> Result<Vec<u8>, String> {
    let mut bytes =
        Vec::with_capacity(vertices.len() * SCENE_LITE_SOLID_QUAD_VERTEX_STRIDE_BYTES as usize);
    for (index, vertex) in vertices.iter().enumerate() {
        if !vertex
            .position
            .into_iter()
            .chain(vertex.rgba)
            .all(f32::is_finite)
        {
            return Err(format!(
                "scene-lite solid quad vertex {index} contains a non-finite value"
            ));
        }
        for value in vertex.position.into_iter().chain(vertex.rgba) {
            bytes.extend_from_slice(&value.to_ne_bytes());
        }
    }
    Ok(bytes)
}

fn scene_lite_sampled_image_vertex_bytes(
    vertices: &[NativeVulkanVulkanaliaSceneLiteSampledImageVertex],
) -> Result<Vec<u8>, String> {
    let mut bytes =
        Vec::with_capacity(vertices.len() * SCENE_LITE_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES as usize);
    for (index, vertex) in vertices.iter().enumerate() {
        if !vertex
            .position
            .into_iter()
            .chain(vertex.uv)
            .chain([vertex.opacity])
            .all(f32::is_finite)
        {
            return Err(format!(
                "scene-lite sampled-image vertex {index} contains a non-finite value"
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

fn scene_lite_solid_quad_index_bytes(
    indices: &[u32],
    vertex_count: usize,
) -> Result<Vec<u8>, String> {
    scene_lite_geometry_index_bytes(indices, vertex_count, "solid quad")
}

fn scene_lite_geometry_index_bytes(
    indices: &[u32],
    vertex_count: usize,
    label: &'static str,
) -> Result<Vec<u8>, String> {
    let max_index = (vertex_count - 1) as u32;
    let mut bytes = Vec::with_capacity(indices.len() * 4);
    for index in indices {
        if *index > max_index {
            return Err(format!(
                "scene-lite {label} index {index} exceeds max vertex index {max_index}"
            ));
        }
        bytes.extend_from_slice(&index.to_ne_bytes());
    }
    Ok(bytes)
}

fn submit_scene_lite_solid_quad_command_buffer2(
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
            .map_err(|err| format!("vkQueueSubmit2(vulkanalia scene-lite present): {err:?}"))?;
    }

    Ok(())
}

fn scene_lite_color_subresource_range() -> vk::ImageSubresourceRange {
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
        let payload = scene_lite_solid_quad_geometry_payload(
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
        let input = scene_lite_solid_quad_full_extent_geometry_input(
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
        let bytes = scene_lite_solid_quad_index_bytes(&input.indices, input.vertices.len())
            .expect("full extent quad indices");
        let indices = bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_ne_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();

        assert_eq!(indices, vec![0, 1, 2, 2, 3, 0]);
    }

    #[test]
    fn solid_quad_geometry_accepts_scene_lite_draw_plan_payload() {
        let input = NativeVulkanVulkanaliaSceneLiteSolidQuadGeometryInput::new(
            vec![
                NativeVulkanVulkanaliaSceneLiteSolidQuadVertex::new(
                    [-160.0, -78.0],
                    [0.2, 0.4, 0.6, 0.75],
                ),
                NativeVulkanVulkanaliaSceneLiteSolidQuadVertex::new(
                    [160.0, -78.0],
                    [0.2, 0.4, 0.6, 0.75],
                ),
                NativeVulkanVulkanaliaSceneLiteSolidQuadVertex::new(
                    [-160.0, 102.0],
                    [0.2, 0.4, 0.6, 0.75],
                ),
                NativeVulkanVulkanaliaSceneLiteSolidQuadVertex::new(
                    [160.0, 102.0],
                    [0.2, 0.4, 0.6, 0.75],
                ),
            ],
            vec![0, 1, 2, 2, 1, 3],
            "scene-lite-runtime-draw-plan",
        );

        let payload = scene_lite_solid_quad_geometry_payload_from_input(&input).unwrap();

        assert_eq!(payload.source_label, "scene-lite-runtime-draw-plan");
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
        let input = NativeVulkanVulkanaliaSceneLiteSolidQuadGeometryInput::new(
            vec![
                NativeVulkanVulkanaliaSceneLiteSolidQuadVertex::new(
                    [0.0, 0.0],
                    [1.0, 0.0, 0.0, 1.0],
                ),
                NativeVulkanVulkanaliaSceneLiteSolidQuadVertex::new(
                    [1.0, 0.0],
                    [1.0, 0.0, 0.0, 1.0],
                ),
                NativeVulkanVulkanaliaSceneLiteSolidQuadVertex::new(
                    [0.0, 1.0],
                    [1.0, 0.0, 0.0, 1.0],
                ),
            ],
            vec![0, 1, 3],
            "bad-geometry",
        );

        let err = scene_lite_solid_quad_geometry_payload_from_input(&input).unwrap_err();

        assert!(err.contains("exceeds max vertex index"));
    }

    #[test]
    fn sampled_image_vertices_cover_full_pixel_extent() {
        let payload = scene_lite_sampled_image_geometry_payload(
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

        let cover = scene_lite_sampled_image_fit_geometry_input(target, source, FitMode::Cover)
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

        let contain = scene_lite_sampled_image_fit_geometry_input(target, source, FitMode::Contain)
            .expect("contain geometry");
        assert_eq!(
            sampled_image_positions(&contain),
            vec![[250.0, 0.0], [750.0, 0.0], [250.0, 500.0], [750.0, 500.0]]
        );

        let stretch = scene_lite_sampled_image_fit_geometry_input(target, source, FitMode::Stretch)
            .expect("stretch geometry");
        assert_eq!(
            sampled_image_positions(&stretch),
            vec![[0.0, 0.0], [1000.0, 0.0], [0.0, 500.0], [1000.0, 500.0]]
        );

        let center = scene_lite_sampled_image_fit_geometry_input(target, source, FitMode::Center)
            .expect("center geometry");
        assert_eq!(
            sampled_image_positions(&center),
            vec![[300.0, 50.0], [700.0, 50.0], [300.0, 450.0], [700.0, 450.0]]
        );

        let tile = scene_lite_sampled_image_fit_geometry_input(target, source, FitMode::Tile)
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
            scene_lite_sampled_image_sampler_mode(Some(FitMode::Tile)),
            NativeVulkanVulkanaliaSceneLiteSampledImageSamplerMode::Repeat
        );
        assert_eq!(
            scene_lite_sampled_image_sampler_mode(Some(FitMode::Cover)),
            NativeVulkanVulkanaliaSceneLiteSampledImageSamplerMode::ClampToEdge
        );
        assert_eq!(
            scene_lite_sampled_image_sampler_mode(None),
            NativeVulkanVulkanaliaSceneLiteSampledImageSamplerMode::ClampToEdge
        );
    }

    fn sampled_image_positions(
        input: &NativeVulkanVulkanaliaSceneLiteSampledImageGeometryInput,
    ) -> Vec<[f32; 2]> {
        input
            .vertices
            .iter()
            .map(|vertex| vertex.position)
            .collect()
    }

    fn sampled_image_uvs(
        input: &NativeVulkanVulkanaliaSceneLiteSampledImageGeometryInput,
    ) -> Vec<[f32; 2]> {
        input.vertices.iter().map(|vertex| vertex.uv).collect()
    }

    #[test]
    fn sampled_image_indices_match_two_triangles() {
        let input = scene_lite_sampled_image_full_extent_geometry_input(vk::Extent2D {
            width: 1000,
            height: 500,
        });
        let bytes = scene_lite_geometry_index_bytes(&input.indices, input.vertices.len(), "test")
            .expect("full extent sampled-image indices");
        let indices = bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_ne_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();

        assert_eq!(indices, vec![0, 1, 2, 2, 1, 3]);
    }

    #[test]
    fn sampled_image_geometry_accepts_scene_lite_draw_plan_payload() {
        let input = NativeVulkanVulkanaliaSceneLiteSampledImageGeometryInput::new(
            vec![
                NativeVulkanVulkanaliaSceneLiteSampledImageVertex::new(
                    [-90.0, -50.0],
                    [0.0, 0.0],
                    0.5,
                ),
                NativeVulkanVulkanaliaSceneLiteSampledImageVertex::new(
                    [110.0, -50.0],
                    [1.0, 0.0],
                    0.5,
                ),
                NativeVulkanVulkanaliaSceneLiteSampledImageVertex::new(
                    [-90.0, 50.0],
                    [0.0, 1.0],
                    0.5,
                ),
                NativeVulkanVulkanaliaSceneLiteSampledImageVertex::new(
                    [110.0, 50.0],
                    [1.0, 1.0],
                    0.5,
                ),
            ],
            vec![0, 1, 2, 2, 1, 3],
            "scene-lite-runtime-sampled-image-draw-plan",
        );

        let payload = scene_lite_sampled_image_geometry_payload_from_input(&input).unwrap();

        assert_eq!(
            payload.source_label,
            "scene-lite-runtime-sampled-image-draw-plan"
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
    fn sampled_image_geometry_accepts_batched_scene_lite_payload() {
        let input = NativeVulkanVulkanaliaSceneLiteSampledImageGeometryInput::new_batched(
            vec![
                NativeVulkanVulkanaliaSceneLiteSampledImageVertex::new([0.0, 0.0], [0.0, 0.0], 1.0),
                NativeVulkanVulkanaliaSceneLiteSampledImageVertex::new(
                    [10.0, 0.0],
                    [1.0, 0.0],
                    1.0,
                ),
                NativeVulkanVulkanaliaSceneLiteSampledImageVertex::new(
                    [0.0, 10.0],
                    [0.0, 1.0],
                    1.0,
                ),
                NativeVulkanVulkanaliaSceneLiteSampledImageVertex::new(
                    [10.0, 10.0],
                    [1.0, 1.0],
                    1.0,
                ),
                NativeVulkanVulkanaliaSceneLiteSampledImageVertex::new(
                    [20.0, 20.0],
                    [0.0, 0.0],
                    0.5,
                ),
                NativeVulkanVulkanaliaSceneLiteSampledImageVertex::new(
                    [30.0, 20.0],
                    [2.0, 0.0],
                    0.5,
                ),
                NativeVulkanVulkanaliaSceneLiteSampledImageVertex::new(
                    [20.0, 30.0],
                    [0.0, 2.0],
                    0.5,
                ),
                NativeVulkanVulkanaliaSceneLiteSampledImageVertex::new(
                    [30.0, 30.0],
                    [2.0, 2.0],
                    0.5,
                ),
            ],
            vec![0, 1, 2, 2, 1, 3, 4, 5, 6, 6, 5, 7],
            vec![PathBuf::from("/tmp/a.png"), PathBuf::from("/tmp/b.png")],
            vec![
                NativeVulkanVulkanaliaSceneLiteSampledImageDrawStep {
                    resource_index: 0,
                    first_index: 0,
                    index_count: 6,
                    fit: Some(FitMode::Cover),
                },
                NativeVulkanVulkanaliaSceneLiteSampledImageDrawStep {
                    resource_index: 1,
                    first_index: 6,
                    index_count: 6,
                    fit: Some(FitMode::Tile),
                },
            ],
            "batched-scene-lite-runtime-sampled-image-draw-plan",
        );

        let payload = scene_lite_sampled_image_geometry_payload_from_input(&input).unwrap();

        assert_eq!(payload.vertex_count, 8);
        assert_eq!(payload.index_count, 12);
        assert_eq!(payload.quad_count, 2);
        assert_eq!(payload.source_count, 2);
        assert_eq!(payload.draw_steps.len(), 2);
        assert_eq!(payload.draw_steps[1].resource_index, 1);
        assert_eq!(payload.draw_steps[1].first_index, 6);
        assert_eq!(payload.draw_steps[1].index_count, 6);
        assert_eq!(
            scene_lite_sampled_image_resource_sampler_mode(1, &payload.draw_steps, None),
            NativeVulkanVulkanaliaSceneLiteSampledImageSamplerMode::Repeat
        );
    }

    #[test]
    fn sampled_image_geometry_rejects_out_of_range_indices() {
        let input = NativeVulkanVulkanaliaSceneLiteSampledImageGeometryInput::new(
            vec![
                NativeVulkanVulkanaliaSceneLiteSampledImageVertex::new([0.0, 0.0], [0.0, 0.0], 1.0),
                NativeVulkanVulkanaliaSceneLiteSampledImageVertex::new([1.0, 0.0], [1.0, 0.0], 1.0),
                NativeVulkanVulkanaliaSceneLiteSampledImageVertex::new([0.0, 1.0], [0.0, 1.0], 1.0),
            ],
            vec![0, 1, 3],
            "bad-sampled-image-geometry",
        );

        let err = scene_lite_sampled_image_geometry_payload_from_input(&input).unwrap_err();

        assert!(err.contains("scene-lite sampled-image index 3 exceeds max vertex index 2"));
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

        let selected = scene_lite_buffer_memory_type_index(
            &memory_types,
            0b11,
            HOST_VISIBLE_COHERENT_DEVICE_LOCAL_MEMORY_FLAG_BITS,
        )
        .unwrap();

        assert_eq!(selected.index, 1);
    }
}
