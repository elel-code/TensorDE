#![allow(dead_code)]

use std::cell::RefCell;
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

use crate::core::{FitMode, SceneBlendMode, SceneSize, SceneTextureRegion};
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
    native_vulkan_vulkanalia_record_scene_sampled_image_draws_inside_rendering,
    native_vulkan_vulkanalia_record_scene_solid_quad_command_buffer,
    native_vulkan_vulkanalia_record_scene_solid_quad_draws_inside_rendering,
};
use super::scene_sampled_image::{
    NativeVulkanVulkanaliaSceneNativeTexture, NativeVulkanVulkanaliaSceneNativeTextureFormat,
    NativeVulkanVulkanaliaSceneSampledImageDescriptorStrategySnapshot,
    NativeVulkanVulkanaliaSceneSampledImageResourceSnapshot,
    NativeVulkanVulkanaliaSceneSampledImageSamplerMode, VulkanaliaSceneSampledImageResources,
    VulkanaliaSceneTransferImageResources,
    native_vulkan_vulkanalia_configure_scene_sampled_image_allocator,
    native_vulkan_vulkanalia_create_scene_sampled_image_resources,
    native_vulkan_vulkanalia_create_scene_transfer_image_resources,
    native_vulkan_vulkanalia_destroy_scene_sampled_image_resources,
    native_vulkan_vulkanalia_destroy_scene_transfer_image_resources,
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
const SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES: u32 = 36;
const SCENE_FULL_SAMPLED_IMAGE_VERTEX_UV_OFFSET_BYTES: usize = 8;
const SCENE_FULL_SAMPLED_IMAGE_VERTEX_UV_BYTES: usize = 8;
const HOST_VISIBLE_COHERENT_MEMORY_FLAG_BITS: u32 =
    vk::MemoryPropertyFlags::HOST_VISIBLE.bits() | vk::MemoryPropertyFlags::HOST_COHERENT.bits();
const HOST_VISIBLE_COHERENT_DEVICE_LOCAL_MEMORY_FLAG_BITS: u32 =
    HOST_VISIBLE_COHERENT_MEMORY_FLAG_BITS | vk::MemoryPropertyFlags::DEVICE_LOCAL.bits();
const HOST_VISIBLE_MEMORY_FLAG_BITS: u32 = vk::MemoryPropertyFlags::HOST_VISIBLE.bits();
const SCENE_GEOMETRY_POOLED_BYTE_BUFFERS: usize = 2;
const SCENE_GEOMETRY_MAX_RETAINED_BYTE_CAPACITY: usize = 128 * 1024;
const SCENE_PRESENT_ID_TELEMETRY_RETAINED_FRAMES: usize = 0;

thread_local! {
    static SCENE_GEOMETRY_BYTE_POOL: RefCell<Vec<Vec<u8>>> = RefCell::new(Vec::new());
}

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

pub(in crate::renderer::native_vulkan) struct NativeVulkanVulkanaliaSceneVideoOverlayInput {
    pub video_geometry: Option<NativeVulkanVulkanaliaSceneVideoLayerGeometryInput>,
    pub source: Option<PathBuf>,
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
    pub tint: [f32; 4],
}

impl NativeVulkanVulkanaliaSceneSampledImageVertex {
    pub fn new(position: [f32; 2], uv: [f32; 2], opacity: f32) -> Self {
        Self::new_tinted(position, uv, opacity, [1.0, 1.0, 1.0, 1.0])
    }

    pub fn new_tinted(position: [f32; 2], uv: [f32; 2], opacity: f32, tint: [f32; 4]) -> Self {
        Self {
            position,
            uv,
            opacity,
            tint,
        }
    }
}

const SCENE_SAMPLED_IMAGE_VERTEX_POOL_MAX_RETAINED: usize = 3;
const SCENE_SAMPLED_IMAGE_VERTEX_POOL_MAX_CAPACITY: usize = 128 * 1024;

thread_local! {
    static SCENE_SAMPLED_IMAGE_VERTEX_POOL:
        RefCell<Vec<Vec<NativeVulkanVulkanaliaSceneSampledImageVertex>>> =
            RefCell::new(Vec::new());
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_take_scene_sampled_image_vertex_vec(
    capacity: usize,
) -> Vec<NativeVulkanVulkanaliaSceneSampledImageVertex> {
    SCENE_SAMPLED_IMAGE_VERTEX_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        let mut vertices = pool
            .iter()
            .position(|vertices| vertices.capacity() >= capacity)
            .map(|index| pool.swap_remove(index))
            .unwrap_or_else(|| Vec::with_capacity(capacity));
        vertices.clear();
        vertices
    })
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_vulkanalia_recycle_scene_sampled_image_vertex_vec(
    mut vertices: Vec<NativeVulkanVulkanaliaSceneSampledImageVertex>,
) {
    if vertices.capacity() > SCENE_SAMPLED_IMAGE_VERTEX_POOL_MAX_CAPACITY {
        return;
    }
    vertices.clear();
    SCENE_SAMPLED_IMAGE_VERTEX_POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        if pool.len() < SCENE_SAMPLED_IMAGE_VERTEX_POOL_MAX_RETAINED {
            pool.push(vertices);
        }
    });
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

#[derive(Debug, Clone, PartialEq)]
pub struct NativeVulkanVulkanaliaSceneVideoLayerGeometryInput {
    pub vertices: Vec<NativeVulkanVulkanaliaSceneSampledImageVertex>,
    pub indices: Vec<u32>,
    pub sources: Vec<PathBuf>,
    pub draw_steps: Vec<NativeVulkanVulkanaliaSceneVideoLayerDrawStep>,
    pub source_label: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeVulkanVulkanaliaSceneSampledImageDrawStep {
    pub layer_index: usize,
    pub resource_index: u32,
    pub first_index: u32,
    pub index_count: u32,
    pub blend_mode: SceneBlendMode,
    pub fit: Option<FitMode>,
    pub texture_region: Option<SceneTextureRegion>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeVulkanVulkanaliaSceneVideoLayerDrawStep {
    pub layer_index: usize,
    pub resource_index: u32,
    pub first_index: u32,
    pub index_count: u32,
    pub fit: Option<FitMode>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeVulkanVulkanaliaSceneSolidQuadDrawStep {
    pub layer_index: usize,
    pub first_index: u32,
    pub index_count: u32,
    pub blend_mode: SceneBlendMode,
}

impl NativeVulkanVulkanaliaSceneVideoLayerGeometryInput {
    pub fn new_batched(
        vertices: Vec<NativeVulkanVulkanaliaSceneSampledImageVertex>,
        indices: Vec<u32>,
        sources: Vec<PathBuf>,
        draw_steps: Vec<NativeVulkanVulkanaliaSceneVideoLayerDrawStep>,
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
                blend_mode: SceneBlendMode::Alpha,
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
                blend_mode: SceneBlendMode::Alpha,
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
    pub retained_frame_telemetry_limit: usize,
    pub present_ids_head: Vec<Option<u64>>,
    pub present_ids_tail: Vec<Option<u64>>,
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
    pub retained_frame_telemetry_limit: usize,
    pub present_ids_head: Vec<Option<u64>>,
    pub present_ids_tail: Vec<Option<u64>>,
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
    animated_uv_steps: Vec<SceneSampledImageAnimatedUvStep>,
    indices: Vec<u32>,
    sources: Vec<PathBuf>,
    snapshot: NativeVulkanVulkanaliaSceneSampledImageGeometrySnapshot,
}

#[derive(Clone, Copy)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneVideoLayerFrameDraw<'a> {
    pub(in crate::renderer::native_vulkan::vulkan) draw_commands:
        &'a [VulkanaliaSceneVideoLayerDrawCommand],
    pub(in crate::renderer::native_vulkan::vulkan) vertex_buffer: vk::Buffer,
    pub(in crate::renderer::native_vulkan::vulkan) index_buffer: vk::Buffer,
}

#[derive(Clone, Copy)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneVideoOverlayFrameDraw<'a> {
    pub(in crate::renderer::native_vulkan::vulkan) video_draw:
        Option<VulkanaliaSceneVideoLayerFrameDraw<'a>>,
    pub(in crate::renderer::native_vulkan::vulkan) overlay_draw:
        Option<VulkanaliaSceneVideoOverlayBlendFrameDraw<'a>>,
}

#[derive(Clone, Copy)]
pub(in crate::renderer::native_vulkan::vulkan) enum VulkanaliaSceneVideoOverlayBlendFrameDraw<'a> {
    Solid {
        draw: VulkanaliaSceneSolidQuadDrawResources<'a>,
    },
    Sampled {
        solid_draw: Option<VulkanaliaSceneSolidQuadDrawResources<'a>>,
        descriptor_heap_draw: VulkanaliaSceneDescriptorHeapDrawResources<'a>,
        pipeline: &'a VulkanaliaSceneSampledImagePipelineResources,
        draw_commands: &'a [VulkanaliaSceneSampledImageDrawCommand],
        vertex_buffer: vk::Buffer,
        index_buffer: vk::Buffer,
    },
}

pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneVideoLayerDrawCommand {
    pub(in crate::renderer::native_vulkan::vulkan) layer_index: usize,
    pub(in crate::renderer::native_vulkan::vulkan) resource_index: u32,
    pub(in crate::renderer::native_vulkan::vulkan) first_index: u32,
    pub(in crate::renderer::native_vulkan::vulkan) index_count: u32,
}

pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneVideoOverlayResources {
    video_geometry: Option<VulkanaliaSceneSampledImageGeometryResources>,
    video_draw_commands: Vec<VulkanaliaSceneVideoLayerDrawCommand>,
    sampled_pipeline: Option<VulkanaliaSceneSampledImagePipelineResources>,
    sampled_geometry: Option<VulkanaliaSceneSampledImageGeometryResources>,
    sampled_images: Vec<VulkanaliaSceneSampledImageResources>,
    descriptor_heap: Option<VulkanaliaDescriptorHeapImageSamplerResources>,
    sampled_draw_commands: Vec<VulkanaliaSceneSampledImageDrawCommand>,
    solid_pipeline: Option<VulkanaliaSceneSolidQuadPipelineResources>,
    solid_geometry: Option<VulkanaliaSceneSolidQuadGeometryResources>,
    solid_draw_commands: Vec<VulkanaliaSceneSolidQuadDrawCommand>,
    dynamic_solid_geometry: Option<NativeVulkanVulkanaliaSceneMixedSolidQuadDynamicGeometry>,
    dynamic_geometry: Option<NativeVulkanVulkanaliaSceneSampledImageDynamicGeometry>,
    scene_size: Option<SceneSize>,
    scene_fit: FitMode,
}

struct VulkanaliaSceneSolidQuadFrameResources {
    swapchain_image_views: Vec<vk::ImageView>,
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    image_available: Vec<vk::Semaphore>,
    render_finished: Vec<vk::Semaphore>,
    in_flight: Vec<vk::Fence>,
}

struct VulkanaliaSceneTransferFrameResources {
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
    memory_size: u64,
    mapped_ptr: Option<*mut std::ffi::c_void>,
    mapped_size: u64,
}

// The mapped pointer belongs to a Vulkan allocation owned by this buffer. Scene
// overlay resources move to the scoped present worker with exclusive &mut access,
// so no concurrent host writes are introduced by making the wrapper Send.
unsafe impl Send for VulkanaliaSceneUploadedBuffer {}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SceneSampledImageAnimatedUvStep {
    vertex_indices: [u32; 4],
    base_uvs: [[f32; 2]; 4],
    texture_region: SceneTextureRegion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SceneStaticTransferBlitRegion {
    src_offsets: [vk::Offset3D; 2],
    dst_offsets: [vk::Offset3D; 2],
    clear_before_blit: bool,
}

struct ScenePresentIdTelemetry {
    head: Vec<Option<u64>>,
    tail: Vec<Option<u64>>,
}

impl ScenePresentIdTelemetry {
    fn new() -> Self {
        Self {
            head: Vec::with_capacity(SCENE_PRESENT_ID_TELEMETRY_RETAINED_FRAMES),
            tail: Vec::with_capacity(SCENE_PRESENT_ID_TELEMETRY_RETAINED_FRAMES),
        }
    }

    fn push(&mut self, present_id: Option<u64>) {
        if SCENE_PRESENT_ID_TELEMETRY_RETAINED_FRAMES == 0 {
            return;
        }
        if self.head.len() < SCENE_PRESENT_ID_TELEMETRY_RETAINED_FRAMES {
            self.head.push(present_id);
        }
        if self.tail.len() == SCENE_PRESENT_ID_TELEMETRY_RETAINED_FRAMES {
            self.tail.remove(0);
        }
        self.tail.push(present_id);
    }

    fn into_parts(self) -> (Vec<Option<u64>>, Vec<Option<u64>>) {
        (self.head, self.tail)
    }
}

#[derive(Debug)]
struct SceneGeometryByteBuffer {
    bytes: Vec<u8>,
}

impl SceneGeometryByteBuffer {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            bytes: take_scene_geometry_byte_buffer(capacity),
        }
    }

    fn len(&self) -> usize {
        self.bytes.len()
    }

    fn extend_from_slice(&mut self, slice: &[u8]) {
        self.bytes.extend_from_slice(slice);
    }
}

impl std::ops::Deref for SceneGeometryByteBuffer {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.bytes
    }
}

impl Drop for SceneGeometryByteBuffer {
    fn drop(&mut self) {
        recycle_scene_geometry_byte_buffer(std::mem::take(&mut self.bytes));
    }
}

fn take_scene_geometry_byte_buffer(capacity: usize) -> Vec<u8> {
    SCENE_GEOMETRY_BYTE_POOL.with(|pool| {
        let mut buffers = pool.borrow_mut();
        let buffer = buffers
            .iter()
            .position(|buffer| buffer.capacity() >= capacity)
            .map(|index| buffers.swap_remove(index));
        let mut buffer = buffer.unwrap_or_else(|| Vec::with_capacity(capacity));
        buffer.clear();
        buffer
    })
}

fn recycle_scene_geometry_byte_buffer(mut bytes: Vec<u8>) {
    if bytes.capacity() > SCENE_GEOMETRY_MAX_RETAINED_BYTE_CAPACITY {
        native_vulkan_vulkanalia_trim_scene_sampled_image_decode_heap();
        return;
    }
    bytes.clear();
    SCENE_GEOMETRY_BYTE_POOL.with(|pool| {
        let mut buffers = pool.borrow_mut();
        if buffers.len() < SCENE_GEOMETRY_POOLED_BYTE_BUFFERS {
            buffers.push(bytes);
        }
    });
}

#[derive(Debug)]
struct VulkanaliaSceneSolidQuadGeometryPayload {
    indices: Vec<u32>,
    vertex_bytes: SceneGeometryByteBuffer,
    index_bytes: SceneGeometryByteBuffer,
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
    vertex_bytes: SceneGeometryByteBuffer,
    index_bytes: SceneGeometryByteBuffer,
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
    native_vulkan_vulkanalia_configure_scene_sampled_image_allocator();

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
    mut options: NativeVulkanVulkanaliaSceneSolidQuadPresentOptions,
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
        options.geometry.take(),
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
        options.dynamic_geometry.is_some(),
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
    mut options: NativeVulkanVulkanaliaSceneSampledImagePresentOptions,
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

    if scene_sampled_image_can_use_static_transfer_present(&options) {
        let present_timing = VulkanaliaPresentTimingConfig::new(
            swapchain_plan.present_id2_enabled,
            swapchain_plan.present_wait2_enabled,
        );
        let result = run_scene_sampled_image_static_transfer_present(
            vulkan,
            instance,
            device,
            selection.physical_device,
            present_device.queue,
            swapchain,
            &swapchain_images,
            selection.queue_family_index,
            &selection,
            &present_device.extension_snapshot,
            &swapchain_plan,
            present_timing,
            options,
            present_device
                .feature_selection
                .core_features
                .texture_compression_bc,
            present_device
                .feature_selection
                .core_features
                .descriptor_heap,
            present_device
                .feature_selection
                .descriptor_heap_properties
                .max_resource_heap_size,
            present_device
                .feature_selection
                .descriptor_heap_properties
                .image_descriptor_size,
            present_device
                .feature_selection
                .descriptor_heap_properties
                .sampler_descriptor_size,
            present_device
                .feature_selection
                .core_features
                .push_descriptor,
            present_device
                .feature_selection
                .vulkan_1_4_properties
                .max_push_descriptors,
        );
        unsafe {
            device.destroy_swapchain_khr(swapchain, None);
            present_device.device.destroy_device(None);
        }
        return result;
    }

    if !present_device.feature_selection.dynamic_rendering_enabled {
        unsafe {
            device.destroy_swapchain_khr(swapchain, None);
            present_device.device.destroy_device(None);
        }
        return Err("Vulkanalia scene sampled image present requires dynamicRendering for CmdBeginRendering".to_owned());
    }

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
            "scene sampled-image runtime requires textureCompressionBC for native BC .gtex resources"
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
        options.geometry.take(),
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
    let retain_sampled_image_dynamic_topology = options.dynamic_geometry.is_some();
    let geometry = match create_scene_sampled_image_geometry_resources(
        device,
        &memory_properties,
        geometry_payload,
        frame_resources.in_flight.len(),
        retain_sampled_image_dynamic_topology,
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
    let solid_geometry_input = options.solid_geometry.take();
    let solid_geometry = if let Some(solid_geometry_input) = solid_geometry_input {
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
            options.dynamic_solid_geometry.is_some(),
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

    let release_static_sources_after_first_present =
        scene_sampled_image_can_release_sources_after_first_present(&options, &geometry);
    let mut pipeline = Some(pipeline);
    let mut geometry = Some(geometry);
    let mut solid_pipeline = solid_pipeline;
    let mut solid_geometry = solid_geometry;
    let mut sampled_images = Some(sampled_images);
    let mut descriptor_heap = descriptor_heap;
    let result = if release_static_sources_after_first_present {
        run_scene_sampled_image_present_loop_release_static_sources(
            vulkan,
            device,
            present_device.queue,
            swapchain,
            &swapchain_images,
            swapchain_plan.extent,
            &frame_resources,
            pipeline
                .take()
                .expect("sampled image pipeline is live before static-source release loop"),
            geometry
                .take()
                .expect("sampled image geometry is live before static-source release loop"),
            solid_pipeline.take(),
            solid_geometry.take(),
            sampled_images
                .take()
                .expect("sampled images are live before static-source release loop"),
            &draw_commands,
            descriptor_heap.take(),
            descriptor_strategy,
            &selection,
            &present_device.extension_snapshot,
            &swapchain_plan,
            present_timing,
            options,
        )
    } else {
        run_scene_sampled_image_present_loop(
            vulkan,
            device,
            present_device.queue,
            swapchain,
            &swapchain_images,
            swapchain_plan.extent,
            &frame_resources,
            pipeline
                .as_ref()
                .expect("sampled image pipeline is live before retained loop"),
            geometry
                .as_ref()
                .expect("sampled image geometry is live before retained loop"),
            solid_pipeline.as_ref(),
            solid_geometry.as_ref(),
            sampled_images
                .as_ref()
                .expect("sampled images are live before retained loop"),
            &draw_commands,
            descriptor_heap.as_ref(),
            descriptor_strategy,
            &selection,
            &present_device.extension_snapshot,
            &swapchain_plan,
            present_timing,
            options,
        )
    };

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
    if let Some(sampled_images) = sampled_images {
        for resource in sampled_images {
            native_vulkan_vulkanalia_destroy_scene_sampled_image_resources(device, resource);
        }
    }
    if let Some(geometry) = geometry {
        destroy_scene_sampled_image_geometry_resources(device, geometry);
    }
    if let Some(pipeline) = pipeline {
        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_resources(device, pipeline);
    }
    destroy_scene_solid_quad_frame_resources(device, frame_resources);
    unsafe {
        device.destroy_swapchain_khr(swapchain, None);
        present_device.device.destroy_device(None);
    }

    result
}

#[allow(clippy::too_many_arguments)]
pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_create_scene_video_overlay_resources(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    command_pool: vk::CommandPool,
    queue: vk::Queue,
    swapchain_format: vk::Format,
    extent: vk::Extent2D,
    frame_resource_count: usize,
    texture_compression_bc_available: bool,
    descriptor_heap_enabled: bool,
    descriptor_heap_properties: super::features::NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
    mut input: NativeVulkanVulkanaliaSceneVideoOverlayInput,
) -> Result<Option<VulkanaliaSceneVideoOverlayResources>, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("scene video overlay requires non-zero extent".to_owned());
    }

    let sampled_image_sources = match (input.source.as_ref(), input.geometry.as_ref()) {
        (_, Some(geometry)) if !geometry.sources.is_empty() => geometry.sources.clone(),
        (Some(source), geometry) => scene_sampled_image_sources(source, geometry),
        (None, _) => Vec::new(),
    };
    let sampled_overlay_requested = !sampled_image_sources.is_empty();
    let solid_overlay_requested = input.solid_geometry.is_some();
    let video_layer_requested = input.video_geometry.is_some();
    if !sampled_overlay_requested && !solid_overlay_requested && !video_layer_requested {
        return Ok(None);
    }

    let mut video_geometry = None;
    let mut video_draw_commands = Vec::new();
    let mut sampled_pipeline = None;
    let mut sampled_geometry = None;
    let mut sampled_images = Vec::new();
    let mut descriptor_heap = None;
    let mut sampled_draw_commands = Vec::new();
    let mut solid_pipeline = None;
    let mut solid_geometry = None;
    let mut solid_draw_commands = Vec::new();

    let result = (|| -> Result<VulkanaliaSceneVideoOverlayResources, String> {
        if let Some(video_geometry_input) = input.video_geometry.take() {
            let geometry_payload = scene_video_layer_geometry_payload(
                video_geometry_input,
                extent,
                input.scene_size,
                input.scene_fit,
            )?;
            video_geometry = Some(create_scene_sampled_image_geometry_resources(
                device,
                memory_properties,
                geometry_payload,
                frame_resource_count,
                false,
            )?);
            video_draw_commands = scene_video_layer_draw_commands(
                &video_geometry
                    .as_ref()
                    .expect("scene video layer geometry is live")
                    .draw_steps,
            )?;
        }

        if sampled_overlay_requested {
            if !texture_compression_bc_available {
                return Err(
                    "scene video overlay requires textureCompressionBC for native BC .gtex resources"
                        .to_owned(),
                );
            }
            if !descriptor_heap_enabled {
                return Err(
                    "scene video overlay requires VK_EXT_descriptor_heap sampled image binding"
                        .to_owned(),
                );
            }
            let descriptor_heap_plan = native_vulkan_vulkanalia_descriptor_heap_image_sampler_plan(
                NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanInput {
                    image_count: sampled_image_sources.len(),
                    properties: descriptor_heap_properties,
                },
            );
            if !descriptor_heap_plan.backend_ready {
                return Err(format!(
                    "scene video overlay descriptor heap is not ready: {:?}",
                    descriptor_heap_plan.blocking_reason
                ));
            }
            sampled_pipeline = Some(
                native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_resources(
                    device,
                    swapchain_format,
                    extent,
                    &descriptor_heap_plan,
                )?,
            );
            let native_textures = scene_sampled_image_load_sources(&sampled_image_sources)?;
            let source_extent = native_textures
                .first()
                .map(|texture| vk::Extent2D {
                    width: texture.width,
                    height: texture.height,
                })
                .ok_or_else(|| "scene video overlay has no sampled image texture".to_owned())?;
            let geometry_payload = scene_sampled_image_geometry_payload(
                input.geometry.take(),
                extent,
                input.fit,
                source_extent,
                input.scene_size,
                input.scene_fit,
            )?;
            sampled_geometry = Some(create_scene_sampled_image_geometry_resources(
                device,
                memory_properties,
                geometry_payload,
                frame_resource_count,
                input.dynamic_geometry.is_some(),
            )?);
            let sampled_geometry_ref = sampled_geometry
                .as_ref()
                .expect("scene video overlay sampled geometry is live");
            for (resource_index, texture) in native_textures.into_iter().enumerate() {
                let resource = native_vulkan_vulkanalia_create_scene_sampled_image_resources(
                    device,
                    memory_properties,
                    command_pool,
                    queue,
                    scene_sampled_image_resource_sampler_mode(
                        resource_index,
                        &sampled_geometry_ref.draw_steps,
                        input.fit,
                    ),
                    texture.source.display().to_string(),
                    &texture,
                )?;
                sampled_images.push(resource);
            }
            native_vulkan_vulkanalia_trim_scene_sampled_image_decode_heap();
            descriptor_heap = Some(create_scene_sampled_image_descriptor_heap_resources(
                device,
                memory_properties,
                &descriptor_heap_plan,
                &sampled_images,
            )?);
            sampled_draw_commands = scene_sampled_image_draw_commands(
                &sampled_geometry_ref.draw_steps,
                &sampled_images,
            )?;
        }

        if solid_overlay_requested {
            solid_pipeline = Some(
                native_vulkan_vulkanalia_create_scene_solid_quad_pipeline_resources(
                    device,
                    swapchain_format,
                    extent,
                )?,
            );
            let geometry_payload = scene_solid_quad_geometry_payload(
                input.solid_geometry.take(),
                extent,
                input.clear_color,
                input.scene_size,
                input.scene_fit,
            )?;
            solid_geometry = Some(create_scene_solid_quad_geometry_resources(
                device,
                memory_properties,
                geometry_payload,
                if input.dynamic_solid_geometry.is_some() {
                    frame_resource_count
                } else {
                    1
                },
                input.dynamic_solid_geometry.is_some(),
            )?);
            solid_draw_commands = scene_solid_quad_draw_commands(
                &solid_geometry
                    .as_ref()
                    .expect("scene video overlay solid geometry is live")
                    .draw_steps,
            )?;
        }

        Ok(VulkanaliaSceneVideoOverlayResources {
            video_geometry: video_geometry.take(),
            video_draw_commands: std::mem::take(&mut video_draw_commands),
            sampled_pipeline: sampled_pipeline.take(),
            sampled_geometry: sampled_geometry.take(),
            sampled_images: std::mem::take(&mut sampled_images),
            descriptor_heap: descriptor_heap.take(),
            sampled_draw_commands: std::mem::take(&mut sampled_draw_commands),
            solid_pipeline: solid_pipeline.take(),
            solid_geometry: solid_geometry.take(),
            solid_draw_commands: std::mem::take(&mut solid_draw_commands),
            dynamic_solid_geometry: input.dynamic_solid_geometry.take(),
            dynamic_geometry: input.dynamic_geometry.take(),
            scene_size: input.scene_size,
            scene_fit: input.scene_fit,
        })
    })();

    if result.is_err() {
        native_vulkan_vulkanalia_destroy_partial_scene_video_overlay_resources(
            device,
            video_geometry,
            sampled_pipeline,
            sampled_geometry,
            sampled_images,
            descriptor_heap,
            solid_pipeline,
            solid_geometry,
        );
    }

    result.map(Some)
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_destroy_scene_video_overlay_resources(
    device: &Device,
    resources: VulkanaliaSceneVideoOverlayResources,
) {
    native_vulkan_vulkanalia_destroy_partial_scene_video_overlay_resources(
        device,
        resources.video_geometry,
        resources.sampled_pipeline,
        resources.sampled_geometry,
        resources.sampled_images,
        resources.descriptor_heap,
        resources.solid_pipeline,
        resources.solid_geometry,
    );
}

fn native_vulkan_vulkanalia_destroy_partial_scene_video_overlay_resources(
    device: &Device,
    video_geometry: Option<VulkanaliaSceneSampledImageGeometryResources>,
    sampled_pipeline: Option<VulkanaliaSceneSampledImagePipelineResources>,
    sampled_geometry: Option<VulkanaliaSceneSampledImageGeometryResources>,
    sampled_images: Vec<VulkanaliaSceneSampledImageResources>,
    descriptor_heap: Option<VulkanaliaDescriptorHeapImageSamplerResources>,
    solid_pipeline: Option<VulkanaliaSceneSolidQuadPipelineResources>,
    solid_geometry: Option<VulkanaliaSceneSolidQuadGeometryResources>,
) {
    if let Some(video_geometry) = video_geometry {
        destroy_scene_sampled_image_geometry_resources(device, video_geometry);
    }
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
    for sampled_image in sampled_images {
        native_vulkan_vulkanalia_destroy_scene_sampled_image_resources(device, sampled_image);
    }
    if let Some(sampled_geometry) = sampled_geometry {
        destroy_scene_sampled_image_geometry_resources(device, sampled_geometry);
    }
    if let Some(sampled_pipeline) = sampled_pipeline {
        native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_resources(
            device,
            sampled_pipeline,
        );
    }
}

impl VulkanaliaSceneVideoOverlayResources {
    pub(in crate::renderer::native_vulkan::vulkan) fn frame_draw(
        &mut self,
        device: &Device,
        frame_slot: usize,
        elapsed_ms: u64,
        extent: vk::Extent2D,
    ) -> Result<Option<VulkanaliaSceneVideoOverlayFrameDraw<'_>>, String> {
        let video_draw = if let Some(geometry) = self.video_geometry.as_ref() {
            if self.video_draw_commands.is_empty() {
                return Err("scene video overlay requires non-empty video layer draws".to_owned());
            }
            Some(VulkanaliaSceneVideoLayerFrameDraw {
                draw_commands: &self.video_draw_commands,
                vertex_buffer: update_scene_sampled_image_geometry_for_time(
                    device, geometry, frame_slot, elapsed_ms, None,
                )?,
                index_buffer: geometry.index_buffer,
            })
        } else {
            None
        };

        let solid_draw = match (self.solid_pipeline.as_ref(), self.solid_geometry.as_ref()) {
            (Some(pipeline), Some(geometry)) => {
                let dynamic_solid_input = self
                    .dynamic_solid_geometry
                    .as_ref()
                    .map(|dynamic_geometry| dynamic_geometry(elapsed_ms))
                    .transpose()?
                    .flatten();
                let vertex_buffer = if let Some(input) = dynamic_solid_input {
                    let vertex_bytes = input
                        .vertices
                        .len()
                        .checked_mul(SCENE_FULL_SOLID_QUAD_VERTEX_STRIDE_BYTES as usize)
                        .ok_or_else(|| {
                            "scene video overlay dynamic solid vertex bytes overflow".to_owned()
                        })?;
                    if input.indices == geometry.indices
                        && input.draw_steps == geometry.draw_steps
                        && vertex_bytes == geometry.snapshot.vertex_buffer_bytes as usize
                    {
                        update_scene_solid_quad_geometry_input_for_time(
                            device,
                            geometry,
                            frame_slot,
                            input,
                            extent,
                            self.scene_size,
                            self.scene_fit,
                        )?
                    } else {
                        update_scene_solid_quad_geometry_for_time(
                            device, geometry, frame_slot, None,
                        )?
                    }
                } else {
                    update_scene_solid_quad_geometry_for_time(device, geometry, frame_slot, None)?
                };
                Some(VulkanaliaSceneSolidQuadDrawResources {
                    pipeline_resources: pipeline,
                    vertex_buffer,
                    index_buffer: geometry.index_buffer,
                    draw_commands: &self.solid_draw_commands,
                })
            }
            (None, None) => None,
            _ => {
                return Err(
                    "scene video overlay requires both solid pipeline and solid geometry"
                        .to_owned(),
                );
            }
        };

        match (
            self.sampled_pipeline.as_ref(),
            self.sampled_geometry.as_ref(),
            self.descriptor_heap.as_ref(),
        ) {
            (Some(pipeline), Some(geometry), Some(descriptor_heap)) => {
                let vertex_buffer = if let Some(dynamic_geometry) = self.dynamic_geometry.as_ref() {
                    update_scene_sampled_image_geometry_input_for_time(
                        device,
                        geometry,
                        frame_slot,
                        dynamic_geometry(elapsed_ms)?,
                        extent,
                        self.scene_size,
                        self.scene_fit,
                    )?
                } else {
                    update_scene_sampled_image_geometry_for_time(
                        device, geometry, frame_slot, elapsed_ms, None,
                    )?
                };
                Ok(Some(VulkanaliaSceneVideoOverlayFrameDraw {
                    video_draw,
                    overlay_draw: Some(VulkanaliaSceneVideoOverlayBlendFrameDraw::Sampled {
                        solid_draw,
                        descriptor_heap_draw: VulkanaliaSceneDescriptorHeapDrawResources {
                            resources: descriptor_heap,
                        },
                        pipeline,
                        draw_commands: &self.sampled_draw_commands,
                        vertex_buffer,
                        index_buffer: geometry.index_buffer,
                    }),
                }))
            }
            (None, None, None) => {
                let overlay_draw = solid_draw
                    .map(|draw| VulkanaliaSceneVideoOverlayBlendFrameDraw::Solid { draw });
                if video_draw.is_some() || overlay_draw.is_some() {
                    Ok(Some(VulkanaliaSceneVideoOverlayFrameDraw {
                        video_draw,
                        overlay_draw,
                    }))
                } else {
                    Ok(None)
                }
            }
            _ => Err("scene video overlay sampled resources are partially initialized".to_owned()),
        }
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_record_scene_video_overlay_draws_inside_rendering(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    extent: vk::Extent2D,
    draw: VulkanaliaSceneVideoOverlayFrameDraw<'_>,
) -> Result<u32, String> {
    match draw.overlay_draw {
        Some(VulkanaliaSceneVideoOverlayBlendFrameDraw::Solid { draw }) => {
            native_vulkan_vulkanalia_record_scene_solid_quad_draws_inside_rendering(
                device,
                command_buffer,
                extent,
                draw,
            )
        }
        Some(VulkanaliaSceneVideoOverlayBlendFrameDraw::Sampled {
            solid_draw,
            descriptor_heap_draw,
            pipeline,
            draw_commands,
            vertex_buffer,
            index_buffer,
        }) => native_vulkan_vulkanalia_record_scene_sampled_image_draws_inside_rendering(
            device,
            command_buffer,
            extent,
            solid_draw,
            Some(descriptor_heap_draw),
            pipeline,
            draw_commands,
            vertex_buffer,
            index_buffer,
        ),
        None => Ok(0),
    }
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
    let mut present_ids = ScenePresentIdTelemetry::new();
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
        let vertex_buffer = if let Some(dynamic_geometry) = options.dynamic_geometry.as_ref() {
            update_scene_solid_quad_geometry_input_for_time(
                device,
                geometry,
                present_frame_slot,
                dynamic_geometry(elapsed_ms)?,
                extent,
                options.scene_size,
                options.scene_fit,
            )?
        } else {
            update_scene_solid_quad_geometry_for_time(device, geometry, present_frame_slot, None)?
        };

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
    let (present_ids_head, present_ids_tail) = present_ids.into_parts();
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
        retained_frame_telemetry_limit: SCENE_PRESENT_ID_TELEMETRY_RETAINED_FRAMES,
        present_ids_head,
        present_ids_tail,
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
    let mut present_ids = ScenePresentIdTelemetry::new();
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
    let mut recorded_commands = vec![None; swapchain_images.len()];

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
        let geometry_frame_slot = image_index_usize;
        let vertex_buffer = if let Some(dynamic_geometry) = options.dynamic_geometry.as_ref() {
            update_scene_sampled_image_geometry_input_for_time(
                device,
                geometry,
                geometry_frame_slot,
                dynamic_geometry(elapsed_ms)?,
                extent,
                options.scene_size,
                options.scene_fit,
            )?
        } else {
            update_scene_sampled_image_geometry_for_time(
                device,
                geometry,
                geometry_frame_slot,
                elapsed_ms,
                None,
            )?
        };
        let dynamic_solid_input = options
            .dynamic_solid_geometry
            .as_ref()
            .map(|dynamic_geometry| dynamic_geometry(elapsed_ms))
            .transpose()?
            .flatten();
        let solid_quad_draw = match (solid_quad_draw.as_ref(), solid_geometry) {
            (Some(draw), Some(geometry)) => {
                let solid_vertex_buffer = if let Some(input) = dynamic_solid_input {
                    update_scene_solid_quad_geometry_input_for_time(
                        device,
                        geometry,
                        geometry_frame_slot,
                        input,
                        extent,
                        options.scene_size,
                        options.scene_fit,
                    )?
                } else {
                    update_scene_solid_quad_geometry_for_time(
                        device,
                        geometry,
                        geometry_frame_slot,
                        None,
                    )?
                };
                Some(VulkanaliaSceneSolidQuadDrawResources {
                    pipeline_resources: draw.pipeline_resources,
                    vertex_buffer: solid_vertex_buffer,
                    index_buffer: draw.index_buffer,
                    draw_commands: draw.draw_commands,
                })
            }
            _ => None,
        };

        let command = if let Some(command) = recorded_commands
            .get(image_index_usize)
            .and_then(Option::as_ref)
            .cloned()
        {
            command
        } else {
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
            if let Some(slot) = recorded_commands.get_mut(image_index_usize) {
                *slot = Some(command.clone());
            }
            command
        };
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
    let (present_ids_head, present_ids_tail) = present_ids.into_parts();
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
        retained_frame_telemetry_limit: SCENE_PRESENT_ID_TELEMETRY_RETAINED_FRAMES,
        present_ids_head,
        present_ids_tail,
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

fn scene_sampled_image_can_release_sources_after_first_present(
    options: &NativeVulkanVulkanaliaSceneSampledImagePresentOptions,
    geometry: &VulkanaliaSceneSampledImageGeometryResources,
) -> bool {
    options.dynamic_geometry.is_none()
        && options.dynamic_solid_geometry.is_none()
        && !scene_sampled_image_draw_steps_are_animated(&geometry.draw_steps)
}

fn scene_sampled_image_can_use_static_transfer_present(
    options: &NativeVulkanVulkanaliaSceneSampledImagePresentOptions,
) -> bool {
    options.solid_geometry.is_none()
        && options.geometry.is_none()
        && options.dynamic_solid_geometry.is_none()
        && options.dynamic_geometry.is_none()
        && matches!(
            options.fit.unwrap_or(FitMode::Stretch),
            FitMode::Cover | FitMode::Contain | FitMode::Stretch | FitMode::Center
        )
}

#[allow(clippy::too_many_arguments)]
fn run_scene_sampled_image_static_transfer_present(
    vulkan: &NativeVulkanVulkanaliaInstance,
    instance: &Instance,
    device: &Device,
    physical_device: vk::PhysicalDevice,
    queue: vk::Queue,
    swapchain: vk::SwapchainKHR,
    swapchain_images: &[vk::Image],
    queue_family_index: u32,
    selection: &super::swapchain::NativeVulkanVulkanaliaPresentQueueSelection,
    extension_snapshot: &NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot,
    swapchain_plan: &super::swapchain::NativeVulkanVulkanaliaSwapchainPlan,
    present_timing: VulkanaliaPresentTimingConfig,
    options: NativeVulkanVulkanaliaSceneSampledImagePresentOptions,
    texture_compression_bc_enabled: bool,
    descriptor_heap_available: bool,
    max_resource_heap_size: u64,
    image_descriptor_size: u64,
    sampler_descriptor_size: u64,
    push_descriptor_available: bool,
    max_push_descriptors: u32,
) -> Result<NativeVulkanVulkanaliaSceneSampledImagePresentSnapshot, String> {
    if !texture_compression_bc_enabled {
        return Err(
            "scene static transfer present requires textureCompressionBC for native BC .gtex resources"
                .to_owned(),
        );
    }

    let hold_started_at = Instant::now();
    let target_duration = options.duration;
    let texture = native_vulkan_vulkanalia_load_scene_native_texture(&options.source)?;
    let filter = scene_static_transfer_blit_filter(
        instance,
        physical_device,
        texture.format,
        swapchain_plan.format.format,
        options.fit.unwrap_or(FitMode::Stretch),
    )?;
    let frame_resources =
        create_scene_transfer_frame_resources(device, swapchain_images, queue_family_index)?;
    let memory_properties =
        unsafe { instance.get_physical_device_memory_properties(physical_device) };
    let transfer_image = match native_vulkan_vulkanalia_create_scene_transfer_image_resources(
        device,
        &memory_properties,
        frame_resources.command_pool,
        queue,
        texture.source.display().to_string(),
        &texture,
    ) {
        Ok(transfer_image) => transfer_image,
        Err(err) => {
            destroy_scene_transfer_frame_resources(device, frame_resources);
            return Err(err);
        }
    };

    let mut result = run_scene_sampled_image_static_transfer_present_loop(
        vulkan,
        device,
        queue,
        swapchain,
        swapchain_images,
        swapchain_plan.extent,
        &frame_resources,
        &transfer_image,
        &texture,
        filter,
        selection,
        extension_snapshot,
        swapchain_plan,
        present_timing,
        options,
        descriptor_heap_available,
        max_resource_heap_size,
        image_descriptor_size,
        sampler_descriptor_size,
        push_descriptor_available,
        max_push_descriptors,
    );

    let _ = unsafe { device.device_wait_idle() };
    native_vulkan_vulkanalia_destroy_scene_transfer_image_resources(device, transfer_image);
    destroy_scene_transfer_frame_resources(device, frame_resources);
    native_vulkan_vulkanalia_trim_scene_sampled_image_decode_heap();
    if let Ok(snapshot) = result.as_mut() {
        let now = Instant::now();
        let deadline = hold_started_at + target_duration;
        if deadline > now {
            thread::sleep(deadline - now);
        }
        let elapsed = hold_started_at.elapsed();
        snapshot.runtime_elapsed_ms = elapsed.as_millis().min(u64::MAX as u128) as u64;
        snapshot.average_present_fps = if elapsed.is_zero() {
            0.0
        } else {
            snapshot.frames_presented as f64 / elapsed.as_secs_f64()
        };
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn run_scene_sampled_image_static_transfer_present_loop(
    vulkan: &NativeVulkanVulkanaliaInstance,
    device: &Device,
    queue: vk::Queue,
    swapchain: vk::SwapchainKHR,
    swapchain_images: &[vk::Image],
    extent: vk::Extent2D,
    frame_resources: &VulkanaliaSceneTransferFrameResources,
    transfer_image: &VulkanaliaSceneTransferImageResources,
    texture: &NativeVulkanVulkanaliaSceneNativeTexture,
    filter: vk::Filter,
    selection: &super::swapchain::NativeVulkanVulkanaliaPresentQueueSelection,
    extension_snapshot: &NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot,
    swapchain_plan: &super::swapchain::NativeVulkanVulkanaliaSwapchainPlan,
    present_timing: VulkanaliaPresentTimingConfig,
    options: NativeVulkanVulkanaliaSceneSampledImagePresentOptions,
    descriptor_heap_available: bool,
    max_resource_heap_size: u64,
    image_descriptor_size: u64,
    sampler_descriptor_size: u64,
    push_descriptor_available: bool,
    max_push_descriptors: u32,
) -> Result<NativeVulkanVulkanaliaSceneSampledImagePresentSnapshot, String> {
    let started_at = Instant::now();
    let source_extent = vk::Extent2D {
        width: texture.width,
        height: texture.height,
    };
    let region = scene_static_transfer_blit_region(
        options.fit.unwrap_or(FitMode::Stretch),
        source_extent,
        extent,
    )?;
    let mut present_ids = ScenePresentIdTelemetry::new();
    let mut present_wait_after_present = false;
    let mut last_command = None;

    let present_result = (|| -> Result<u64, String> {
        let present_frame_slot = 0usize;
        let image_available = frame_resources.image_available[present_frame_slot];
        let render_finished = frame_resources.render_finished[present_frame_slot];
        let in_flight = frame_resources.in_flight[present_frame_slot];
        unsafe {
            device
                .wait_for_fences(&[in_flight], true, u64::MAX)
                .map_err(|err| {
                    format!("vkWaitForFences(vulkanalia scene static transfer present): {err:?}")
                })?;
            device.reset_fences(&[in_flight]).map_err(|err| {
                format!("vkResetFences(vulkanalia scene static transfer present): {err:?}")
            })?;
        }

        let (image_index, _) = unsafe {
            device.acquire_next_image_khr(swapchain, u64::MAX, image_available, vk::Fence::null())
        }
        .map_err(|err| {
            format!("vkAcquireNextImageKHR(vulkanalia scene static transfer present): {err:?}")
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

        let command = record_scene_sampled_image_static_transfer_command_buffer(
            device,
            command_buffer,
            swapchain_image,
            extent,
            transfer_image.image,
            region,
            filter,
            [
                options.clear_color.r,
                options.clear_color.g,
                options.clear_color.b,
                options.clear_color.a,
            ],
        )?;
        submit_scene_transfer_command_buffer2(
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
        let present_id = present_timing.present_id(0);
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
                    format!("vkQueuePresentKHR(vulkanalia scene static transfer present): {err:?}")
                })?;
            device
                .wait_for_fences(&[in_flight], true, u64::MAX)
                .map_err(|err| {
                    format!(
                        "vkWaitForFences(vulkanalia scene static transfer source release): {err:?}"
                    )
                })?;
        }
        present_wait_after_present |= present_timing.wait_after_queue_present(
            device,
            swapchain,
            present_id,
            "scene static transfer present",
        )?;

        present_ids.push(present_id);
        last_command = Some(command);
        Ok(1)
    })();

    let frames_presented = present_result?;
    let elapsed = started_at.elapsed();
    let (present_ids_head, present_ids_tail) = present_ids.into_parts();
    let sampled_image_snapshot = transfer_image.snapshot.clone();
    Ok(NativeVulkanVulkanaliaSceneSampledImagePresentSnapshot {
        binding: "vulkanalia",
        route: "scene-static-transfer-visible-present",
        scene_input_model: "static native .gtex BC source; no retained scene snapshot or CPU decoded image",
        scene_resource_model: "static-transfer-first-present-source-release",
        scene_solid_quad_draw_count: 0,
        scene_sampled_image_resource_count: 1,
        scene_sampled_image_descriptor_heap_required: false,
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
        mixed_scene_draw_enabled: false,
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
        solid_geometry: None,
        solid_pipeline: None,
        geometry: scene_static_transfer_geometry_snapshot(),
        sampled_image: sampled_image_snapshot.clone(),
        sampled_images: vec![sampled_image_snapshot],
        descriptor_strategy: scene_static_transfer_descriptor_strategy(
            descriptor_heap_available,
            max_resource_heap_size,
            image_descriptor_size,
            sampler_descriptor_size,
            push_descriptor_available,
            max_push_descriptors,
        ),
        descriptor_heap: None,
        pipeline: scene_static_transfer_pipeline_snapshot(swapchain_plan.format.format, extent),
        last_command,
        command_submit_model: "acquire_next_image_khr -> cmd_blit_image2 static BC transfer image into swapchain -> queue_submit2 -> queue_present_khr -> wait render fence -> destroy source transfer image -> sleep until duration",
        present_sync_model: "static first-present transfer source release; swapchain/display owns the visible result after submit fence",
        wait_idle_after_present: false,
        retained_frame_telemetry_limit: SCENE_PRESENT_ID_TELEMETRY_RETAINED_FRAMES,
        present_ids_head,
        present_ids_tail,
        uses_present_id2: present_timing.present_id2_enabled,
        present_wait2_available: present_timing.present_wait2_enabled,
        present_wait_after_present,
        uses_pipeline_rendering_create_info: false,
        uses_dynamic_rendering: false,
        uses_synchronization2: true,
        uses_submit2: true,
        zero_copy_scope: "static source image is uploaded as native BC into a transfer-src image, blitted into the swapchain once, then source image memory is destroyed; no CPU image payload is retained",
        primary_reference: "FFmpeg packet/bitstream lifetime discipline: source payload/resource is released after the submitted work no longer references it",
    })
}

fn scene_static_transfer_blit_filter(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    source_format: NativeVulkanVulkanaliaSceneNativeTextureFormat,
    swapchain_format: vk::Format,
    fit: FitMode,
) -> Result<vk::Filter, String> {
    let source = unsafe {
        instance.get_physical_device_format_properties(physical_device, source_format.vk_format())
    };
    if !source
        .optimal_tiling_features
        .contains(vk::FormatFeatureFlags::BLIT_SRC)
    {
        return Err(format!(
            "scene static transfer present requires {} optimal tiling BLIT_SRC",
            source_format.label()
        ));
    }
    let target = unsafe {
        instance.get_physical_device_format_properties(physical_device, swapchain_format)
    };
    if !target
        .optimal_tiling_features
        .contains(vk::FormatFeatureFlags::BLIT_DST)
    {
        return Err(format!(
            "scene static transfer present requires swapchain format {swapchain_format:?} optimal tiling BLIT_DST"
        ));
    }
    let needs_scaling = fit != FitMode::Center;
    let linear_ok = source
        .optimal_tiling_features
        .contains(vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR);
    Ok(if needs_scaling && linear_ok {
        vk::Filter::LINEAR
    } else {
        vk::Filter::NEAREST
    })
}

fn scene_static_transfer_blit_region(
    fit: FitMode,
    source: vk::Extent2D,
    target: vk::Extent2D,
) -> Result<SceneStaticTransferBlitRegion, String> {
    if source.width == 0 || source.height == 0 || target.width == 0 || target.height == 0 {
        return Err(
            "scene static transfer blit requires non-zero source and target extent".to_owned(),
        );
    }
    let src_full = vk::Offset3D {
        x: source.width as i32,
        y: source.height as i32,
        z: 1,
    };
    let dst_full = vk::Offset3D {
        x: target.width as i32,
        y: target.height as i32,
        z: 1,
    };
    let region = match fit {
        FitMode::Stretch => SceneStaticTransferBlitRegion {
            src_offsets: [vk::Offset3D { x: 0, y: 0, z: 0 }, src_full],
            dst_offsets: [vk::Offset3D { x: 0, y: 0, z: 0 }, dst_full],
            clear_before_blit: false,
        },
        FitMode::Center => {
            let copy_width = source.width.min(target.width);
            let copy_height = source.height.min(target.height);
            let src_x = ((source.width - copy_width) / 2) & !3;
            let src_y = ((source.height - copy_height) / 2) & !3;
            let dst_x = ((target.width - copy_width) / 2) as i32;
            let dst_y = ((target.height - copy_height) / 2) as i32;
            SceneStaticTransferBlitRegion {
                src_offsets: [
                    vk::Offset3D {
                        x: src_x as i32,
                        y: src_y as i32,
                        z: 0,
                    },
                    vk::Offset3D {
                        x: (src_x + copy_width) as i32,
                        y: (src_y + copy_height) as i32,
                        z: 1,
                    },
                ],
                dst_offsets: [
                    vk::Offset3D {
                        x: dst_x,
                        y: dst_y,
                        z: 0,
                    },
                    vk::Offset3D {
                        x: dst_x + copy_width as i32,
                        y: dst_y + copy_height as i32,
                        z: 1,
                    },
                ],
                clear_before_blit: copy_width < target.width || copy_height < target.height,
            }
        }
        FitMode::Contain => {
            let scale = (target.width as f64 / source.width as f64)
                .min(target.height as f64 / source.height as f64);
            let dst_width = ((source.width as f64 * scale).round() as u32).clamp(1, target.width);
            let dst_height =
                ((source.height as f64 * scale).round() as u32).clamp(1, target.height);
            let dst_x = ((target.width - dst_width) / 2) as i32;
            let dst_y = ((target.height - dst_height) / 2) as i32;
            SceneStaticTransferBlitRegion {
                src_offsets: [vk::Offset3D { x: 0, y: 0, z: 0 }, src_full],
                dst_offsets: [
                    vk::Offset3D {
                        x: dst_x,
                        y: dst_y,
                        z: 0,
                    },
                    vk::Offset3D {
                        x: dst_x + dst_width as i32,
                        y: dst_y + dst_height as i32,
                        z: 1,
                    },
                ],
                clear_before_blit: dst_width < target.width || dst_height < target.height,
            }
        }
        FitMode::Cover => {
            let source_aspect = source.width as f64 / source.height as f64;
            let target_aspect = target.width as f64 / target.height as f64;
            let (src_x, src_y, src_width, src_height) = if source_aspect > target_aspect {
                let width =
                    ((source.height as f64 * target_aspect).round() as u32).clamp(1, source.width);
                (
                    ((source.width - width) / 2) & !3,
                    0,
                    width & !3,
                    source.height,
                )
            } else {
                let height =
                    ((source.width as f64 / target_aspect).round() as u32).clamp(1, source.height);
                (
                    0,
                    ((source.height - height) / 2) & !3,
                    source.width,
                    height & !3,
                )
            };
            SceneStaticTransferBlitRegion {
                src_offsets: [
                    vk::Offset3D {
                        x: src_x as i32,
                        y: src_y as i32,
                        z: 0,
                    },
                    vk::Offset3D {
                        x: (src_x + src_width.max(4).min(source.width - src_x)) as i32,
                        y: (src_y + src_height.max(4).min(source.height - src_y)) as i32,
                        z: 1,
                    },
                ],
                dst_offsets: [vk::Offset3D { x: 0, y: 0, z: 0 }, dst_full],
                clear_before_blit: false,
            }
        }
        FitMode::Tile => {
            return Err("scene static transfer present does not implement tile repeat; tile remains on the retained sampled-image render path".to_owned());
        }
    };
    Ok(region)
}

#[allow(clippy::too_many_arguments)]
fn record_scene_sampled_image_static_transfer_command_buffer(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    swapchain_image: vk::Image,
    extent: vk::Extent2D,
    source_image: vk::Image,
    region: SceneStaticTransferBlitRegion,
    filter: vk::Filter,
    clear_color: [f32; 4],
) -> Result<NativeVulkanVulkanaliaSceneSampledImageCommandSnapshot, String> {
    unsafe {
        device
            .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
            .map_err(|err| {
                format!("vkResetCommandBuffer(vulkanalia scene static transfer): {err:?}")
            })?;
        let begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
            .build();
        device
            .begin_command_buffer(command_buffer, &begin_info)
            .map_err(|err| {
                format!("vkBeginCommandBuffer(vulkanalia scene static transfer): {err:?}")
            })?;

        let swapchain_to_transfer = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE)
            .src_access_mask(vk::AccessFlags2::empty())
            .dst_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(swapchain_image)
            .subresource_range(scene_color_subresource_range())
            .build();
        let barriers = [swapchain_to_transfer];
        let dependency = vk::DependencyInfo::builder()
            .image_memory_barriers(&barriers)
            .build();
        device.cmd_pipeline_barrier2(command_buffer, &dependency);

        if region.clear_before_blit {
            let clear = vk::ClearColorValue {
                float32: clear_color,
            };
            let ranges = [scene_color_subresource_range()];
            device.cmd_clear_color_image(
                command_buffer,
                swapchain_image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &clear,
                &ranges,
            );
        }

        let subresource = vk::ImageSubresourceLayers::builder()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .mip_level(0)
            .base_array_layer(0)
            .layer_count(1)
            .build();
        let blit = vk::ImageBlit2::builder()
            .src_subresource(subresource)
            .src_offsets(region.src_offsets)
            .dst_subresource(subresource)
            .dst_offsets(region.dst_offsets)
            .build();
        let blits = [blit];
        let blit_info = vk::BlitImageInfo2::builder()
            .src_image(source_image)
            .src_image_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .dst_image(swapchain_image)
            .dst_image_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .regions(&blits)
            .filter(filter)
            .build();
        device.cmd_blit_image2(command_buffer, &blit_info);

        let swapchain_to_present = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
            .src_access_mask(vk::AccessFlags2::TRANSFER_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::BOTTOM_OF_PIPE)
            .dst_access_mask(vk::AccessFlags2::empty())
            .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(swapchain_image)
            .subresource_range(scene_color_subresource_range())
            .build();
        let present_barriers = [swapchain_to_present];
        let present_dependency = vk::DependencyInfo::builder()
            .image_memory_barriers(&present_barriers)
            .build();
        device.cmd_pipeline_barrier2(command_buffer, &present_dependency);

        device.end_command_buffer(command_buffer).map_err(|err| {
            format!("vkEndCommandBuffer(vulkanalia scene static transfer): {err:?}")
        })?;
    }

    Ok(NativeVulkanVulkanaliaSceneSampledImageCommandSnapshot {
        binding: "vulkanalia",
        route: "scene-static-transfer-blit-command",
        extent: (extent.width, extent.height),
        index_count: 0,
        command_buffer_recorded: true,
        vertex_buffer_bound: false,
        index_buffer_bound: false,
        draw_call_count: 0,
        solid_quad_draw_call_count: 0,
        sampled_image_draw_call_count: 0,
        pipeline_bind_count: 0,
        descriptor_set_bound: false,
        push_descriptor_set_recorded: false,
        descriptor_heap_bound: false,
        descriptor_set_bind_count: 0,
        push_descriptor_set_recorded_count: 0,
        descriptor_heap_draw_count: 0,
        descriptor_model: "none-transfer-only",
        push_constant_bytes: 0,
        swapchain_layout_transition: "undefined -> transfer-dst-optimal -> present-src-khr",
        sampled_image_layout: "transfer-src-optimal",
        render_model: "retained BC transfer-src image -> cmd_blit_image2 -> Wayland swapchain; no graphics pipeline or CPU RGBA payload",
        command_order: vec![
            "cmd_pipeline_barrier2_swapchain_transfer_dst",
            "cmd_clear_color_image_when_letterboxed",
            "cmd_blit_image2_static_bc_to_swapchain",
            "cmd_pipeline_barrier2_present",
            "queue_submit2_present",
            "queue_present_khr",
        ],
        uses_dynamic_rendering: false,
        uses_synchronization2: true,
    })
}

fn scene_static_transfer_geometry_snapshot()
-> NativeVulkanVulkanaliaSceneSampledImageGeometrySnapshot {
    NativeVulkanVulkanaliaSceneSampledImageGeometrySnapshot {
        source_label: "static-transfer-no-host-geometry".to_owned(),
        vertex_count: 0,
        vertex_buffer_bytes: 0,
        vertex_buffer_count: 0,
        index_buffer_bytes: 0,
        index_count: 0,
        quad_count: 0,
        source_count: 1,
        draw_step_count: 0,
        vertex_stride_bytes: 0,
        selected_vertex_memory_type_index: 0,
        selected_index_memory_type_index: 0,
        vertex_memory_property_flags: Vec::new(),
        index_memory_property_flags: Vec::new(),
        upload_model: "static transfer path records no host-visible scene geometry buffers",
        retained_across_frames: false,
    }
}

fn scene_static_transfer_descriptor_strategy(
    descriptor_heap_available: bool,
    max_resource_heap_size: u64,
    image_descriptor_size: u64,
    sampler_descriptor_size: u64,
    push_descriptor_available: bool,
    max_push_descriptors: u32,
) -> NativeVulkanVulkanaliaSceneSampledImageDescriptorStrategySnapshot {
    NativeVulkanVulkanaliaSceneSampledImageDescriptorStrategySnapshot {
        binding: "vulkanalia",
        route: "scene-static-transfer-descriptor-strategy",
        sampled_image_count: 1,
        descriptor_set_path_enabled: false,
        active_descriptor_model: "none-transfer-only",
        descriptor_heap_available,
        descriptor_heap_fast_path_candidate: false,
        uses_descriptor_heap_primary_path: false,
        max_resource_heap_size,
        image_descriptor_size,
        sampler_descriptor_size,
        push_descriptor_available,
        max_push_descriptors,
        push_descriptor_fast_path_candidate: false,
        uses_push_descriptor_fast_path: false,
        next_gate: "dynamic scene sampled-image layers continue to use descriptor heap render path",
        primary_reference: "video-style resource lifetime: transfer source is destroyed after the submitted present no longer references it",
    }
}

fn scene_static_transfer_pipeline_snapshot(
    target_format: vk::Format,
    extent: vk::Extent2D,
) -> NativeVulkanVulkanaliaSceneSampledImagePipelineSnapshot {
    NativeVulkanVulkanaliaSceneSampledImagePipelineSnapshot {
        binding: "vulkanalia",
        route: "scene-static-transfer-no-graphics-pipeline",
        target_format: format!("{target_format:?}"),
        extent: (extent.width, extent.height),
        shader_modules_created: false,
        descriptor_set_layout_created: false,
        pipeline_layout_created: false,
        pipeline_created: false,
        render_pass_compatibility: "not-used-transfer-only",
        primitive_topology: "none-transfer-only",
        vertex_input_binding_count: 0,
        vertex_input_attribute_count: 0,
        vertex_stride_bytes: 0,
        vertex_position_format: "none",
        vertex_uv_format: "none",
        vertex_opacity_format: "none",
        vertex_tint_format: "none",
        descriptor_set_count: 0,
        descriptor_model: "none-transfer-only",
        descriptor_heap_mapping_enabled: false,
        descriptor_heap_pipeline_flag_enabled: false,
        descriptor_set_layout_create_flags: Vec::new(),
        descriptor_type: "none",
        descriptor_binding: 0,
        push_constant_bytes: 0,
        push_constant_model: "none-transfer-only",
        blend_model: "none-transfer-only",
        sampled_image_model: "BC transfer-src image copied to swapchain with cmd_blit_image2",
        uses_pipeline_rendering_create_info: false,
        uses_dynamic_rendering: false,
        uses_synchronization2: true,
        uses_submit2: true,
        uses_push_descriptor_fast_path: false,
    }
}

#[allow(clippy::too_many_arguments)]
fn run_scene_sampled_image_present_loop_release_static_sources(
    vulkan: &NativeVulkanVulkanaliaInstance,
    device: &Device,
    queue: vk::Queue,
    swapchain: vk::SwapchainKHR,
    swapchain_images: &[vk::Image],
    extent: vk::Extent2D,
    frame_resources: &VulkanaliaSceneSolidQuadFrameResources,
    pipeline: VulkanaliaSceneSampledImagePipelineResources,
    geometry: VulkanaliaSceneSampledImageGeometryResources,
    solid_pipeline: Option<VulkanaliaSceneSolidQuadPipelineResources>,
    solid_geometry: Option<VulkanaliaSceneSolidQuadGeometryResources>,
    mut sampled_images: Vec<VulkanaliaSceneSampledImageResources>,
    draw_commands: &[VulkanaliaSceneSampledImageDrawCommand],
    mut descriptor_heap: Option<VulkanaliaDescriptorHeapImageSamplerResources>,
    descriptor_strategy: NativeVulkanVulkanaliaSceneSampledImageDescriptorStrategySnapshot,
    selection: &super::swapchain::NativeVulkanVulkanaliaPresentQueueSelection,
    extension_snapshot: &NativeVulkanVulkanaliaPresentDeviceExtensionSnapshot,
    swapchain_plan: &super::swapchain::NativeVulkanVulkanaliaSwapchainPlan,
    present_timing: VulkanaliaPresentTimingConfig,
    options: NativeVulkanVulkanaliaSceneSampledImagePresentOptions,
) -> Result<NativeVulkanVulkanaliaSceneSampledImagePresentSnapshot, String> {
    let started_at = Instant::now();
    let deadline = started_at + options.duration;
    let sampled_image = sampled_images.first().ok_or_else(|| {
        "scene sampled image present requires at least one sampled image".to_owned()
    })?;
    let mut sampled_image_snapshot = sampled_image.snapshot.clone();
    sampled_image_snapshot.retained_across_present_frames = false;
    let sampled_images_snapshot = sampled_images
        .iter()
        .map(|resource| {
            let mut snapshot = resource.snapshot.clone();
            snapshot.retained_across_present_frames = false;
            snapshot
        })
        .collect::<Vec<_>>();
    let descriptor_heap_snapshot = descriptor_heap.as_ref().map(|resources| {
        let mut snapshot = resources.snapshot.clone();
        snapshot.route = "scene-descriptor-heap-image-sampler-first-present-resource";
        snapshot.zero_copy_gate =
            "scene static sampled-image descriptors are consumed by the first present and destroyed after the render fence";
        snapshot
    });
    let sampled_image_resource_count = sampled_images.len().min(u32::MAX as usize) as u32;
    let mut geometry_snapshot = geometry.snapshot.clone();
    geometry_snapshot.retained_across_frames = false;
    geometry_snapshot.upload_model =
        "static first-present host-visible sampled-image geometry destroyed after render fence";
    let pipeline_snapshot = pipeline.snapshot.clone();
    let solid_geometry_snapshot = solid_geometry.as_ref().map(|geometry| {
        let mut snapshot = geometry.snapshot.clone();
        snapshot.retained_across_frames = false;
        snapshot.upload_model =
            "static first-present host-visible solid-quad geometry destroyed after render fence";
        snapshot
    });
    let solid_pipeline_snapshot = solid_pipeline
        .as_ref()
        .map(|pipeline| pipeline.snapshot.clone());
    let mixed_scene_draw_enabled = solid_geometry_snapshot.is_some();
    let scene_solid_quad_draw_count = solid_geometry_snapshot
        .as_ref()
        .map(|geometry| geometry.draw_step_count)
        .unwrap_or(0);

    let solid_draw_commands = match solid_geometry.as_ref() {
        Some(geometry) => Some(scene_solid_quad_draw_commands(&geometry.draw_steps)?),
        None => None,
    };

    let mut present_ids = ScenePresentIdTelemetry::new();
    let mut present_wait_after_present = false;
    let mut last_command = None;
    let present_result = (|| -> Result<u64, String> {
        let present_frame_slot = 0usize;
        let image_available = frame_resources.image_available[present_frame_slot];
        let render_finished = frame_resources.render_finished[present_frame_slot];
        let in_flight = frame_resources.in_flight[present_frame_slot];
        unsafe {
            device
                .wait_for_fences(&[in_flight], true, u64::MAX)
                .map_err(|err| {
                    format!(
                        "vkWaitForFences(vulkanalia scene sampled image static present): {err:?}"
                    )
                })?;
            device.reset_fences(&[in_flight]).map_err(|err| {
                format!("vkResetFences(vulkanalia scene sampled image static present): {err:?}")
            })?;
        }

        let (image_index, _) = unsafe {
            device.acquire_next_image_khr(swapchain, u64::MAX, image_available, vk::Fence::null())
        }
        .map_err(|err| {
            format!("vkAcquireNextImageKHR(vulkanalia scene sampled image static present): {err:?}")
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

        let vertex_buffer =
            update_scene_sampled_image_geometry_for_time(device, &geometry, 0, 0, None)?;
        let solid_quad_draw = match (
            solid_pipeline.as_ref(),
            solid_geometry.as_ref(),
            solid_draw_commands.as_deref(),
        ) {
            (Some(pipeline_resources), Some(geometry), Some(draw_commands)) => {
                let solid_vertex_buffer =
                    update_scene_solid_quad_geometry_for_time(device, geometry, 0, None)?;
                Some(VulkanaliaSceneSolidQuadDrawResources {
                    pipeline_resources,
                    vertex_buffer: solid_vertex_buffer,
                    index_buffer: geometry.index_buffer,
                    draw_commands,
                })
            }
            (None, None, None) => None,
            _ => {
                return Err(
                    "scene mixed present requires both solid pipeline and solid geometry"
                        .to_owned(),
                );
            }
        };
        let descriptor_heap_draw = descriptor_heap
            .as_ref()
            .map(|resources| VulkanaliaSceneDescriptorHeapDrawResources { resources });

        let command = native_vulkan_vulkanalia_record_scene_sampled_image_command_buffer(
            device,
            command_buffer,
            swapchain_image,
            swapchain_view,
            extent,
            solid_quad_draw,
            descriptor_heap_draw,
            &pipeline,
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
        let present_id = present_timing.present_id(0);
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
                    format!(
                        "vkQueuePresentKHR(vulkanalia scene sampled image static present): {err:?}"
                    )
                })?;
            device
                .wait_for_fences(&[in_flight], true, u64::MAX)
                .map_err(|err| {
                    format!(
                        "vkWaitForFences(vulkanalia scene sampled image static source release): {err:?}"
                    )
                })?;
        }
        present_wait_after_present |= present_timing.wait_after_queue_present(
            device,
            swapchain,
            present_id,
            "scene sampled image static present",
        )?;

        present_ids.push(present_id);
        last_command = Some(command);
        Ok(1)
    })();

    let _ = unsafe { device.device_wait_idle() };
    if let Some(descriptor_heap) = descriptor_heap.take() {
        native_vulkan_vulkanalia_destroy_descriptor_heap_image_sampler_resources(
            device,
            descriptor_heap,
        );
    }
    for resource in sampled_images.drain(..) {
        native_vulkan_vulkanalia_destroy_scene_sampled_image_resources(device, resource);
    }
    if let Some(solid_geometry) = solid_geometry {
        destroy_scene_solid_quad_geometry_resources(device, solid_geometry);
    }
    if let Some(solid_pipeline) = solid_pipeline {
        native_vulkan_vulkanalia_destroy_scene_solid_quad_pipeline_resources(
            device,
            solid_pipeline,
        );
    }
    destroy_scene_sampled_image_geometry_resources(device, geometry);
    native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_resources(device, pipeline);
    native_vulkan_vulkanalia_trim_scene_sampled_image_decode_heap();

    let frames_presented = present_result?;
    let now = Instant::now();
    if deadline > now {
        thread::sleep(deadline - now);
    }

    let elapsed = started_at.elapsed();
    let (present_ids_head, present_ids_tail) = present_ids.into_parts();
    Ok(NativeVulkanVulkanaliaSceneSampledImagePresentSnapshot {
        binding: "vulkanalia",
        route: "scene-sampled-image-visible-present",
        scene_input_model: "core scene snapshot layers; groups must be flattened before native Vulkan planning",
        scene_resource_model: if mixed_scene_draw_enabled {
            "static-first-present-source-release-solid-quad-geometry"
        } else {
            "static-first-present-source-release"
        },
        scene_solid_quad_draw_count,
        scene_sampled_image_resource_count: sampled_image_resource_count,
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
        mixed_scene_draw_enabled,
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
        solid_geometry: solid_geometry_snapshot,
        solid_pipeline: solid_pipeline_snapshot,
        geometry: geometry_snapshot,
        sampled_image: sampled_image_snapshot,
        sampled_images: sampled_images_snapshot,
        descriptor_strategy,
        descriptor_heap: descriptor_heap_snapshot,
        pipeline: pipeline_snapshot,
        last_command,
        command_submit_model: "acquire_next_image_khr -> cmd_begin_rendering sampled image quad -> queue_submit2 -> queue_present_khr -> wait render fence -> destroy source sampled images/descriptors -> sleep until duration",
        present_sync_model: "static first-present source release; swapchain/display owns the visible result after render fence",
        wait_idle_after_present: false,
        retained_frame_telemetry_limit: SCENE_PRESENT_ID_TELEMETRY_RETAINED_FRAMES,
        present_ids_head,
        present_ids_tail,
        uses_present_id2: present_timing.present_id2_enabled,
        present_wait2_available: present_timing.present_wait2_enabled,
        present_wait_after_present,
        uses_pipeline_rendering_create_info: true,
        uses_dynamic_rendering: true,
        uses_synchronization2: true,
        uses_submit2: true,
        zero_copy_scope: "static source image is sampled for the first completed present only, then descriptor heap and sampled image memory are destroyed while the swapchain keeps the displayed contents",
        primary_reference: "FFmpeg packet/bitstream lifetime discipline: source payload/resource is released after the submitted work no longer references it",
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

fn create_scene_transfer_frame_resources(
    device: &Device,
    swapchain_images: &[vk::Image],
    queue_family_index: u32,
) -> Result<VulkanaliaSceneTransferFrameResources, String> {
    if swapchain_images.is_empty() {
        return Err("scene transfer present requires at least one swapchain image".to_owned());
    }

    let mut command_pool = vk::CommandPool::null();
    let mut image_available = Vec::new();
    let mut render_finished = Vec::new();
    let mut in_flight = Vec::new();

    let result = (|| -> Result<VulkanaliaSceneTransferFrameResources, String> {
        let command_pool_info = vk::CommandPoolCreateInfo::builder()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(queue_family_index);
        command_pool =
            unsafe { device.create_command_pool(&command_pool_info, None) }.map_err(|err| {
                format!("vkCreateCommandPool(vulkanalia scene transfer present): {err:?}")
            })?;
        let command_buffer_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(swapchain_images.len() as u32);
        let command_buffers = unsafe { device.allocate_command_buffers(&command_buffer_info) }
            .map_err(|err| {
                format!("vkAllocateCommandBuffers(vulkanalia scene transfer present): {err:?}")
            })?;

        let semaphore_info = vk::SemaphoreCreateInfo::builder();
        let fence_info = vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);
        for frame_slot in 0..swapchain_images.len() {
            image_available.push(
                unsafe { device.create_semaphore(&semaphore_info, None) }.map_err(|err| {
                    format!(
                        "vkCreateSemaphore(image_available slot {frame_slot} vulkanalia scene transfer present): {err:?}"
                    )
                })?,
            );
            render_finished.push(
                unsafe { device.create_semaphore(&semaphore_info, None) }.map_err(|err| {
                    format!(
                        "vkCreateSemaphore(render_finished slot {frame_slot} vulkanalia scene transfer present): {err:?}"
                    )
                })?,
            );
            in_flight.push(
                unsafe { device.create_fence(&fence_info, None) }.map_err(|err| {
                    format!("vkCreateFence(slot {frame_slot} vulkanalia scene transfer present): {err:?}")
                })?,
            );
        }

        Ok(VulkanaliaSceneTransferFrameResources {
            command_pool,
            command_buffers,
            image_available: std::mem::take(&mut image_available),
            render_finished: std::mem::take(&mut render_finished),
            in_flight: std::mem::take(&mut in_flight),
        })
    })();

    if result.is_err() {
        destroy_partial_scene_transfer_frame_resources(
            device,
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

fn destroy_scene_transfer_frame_resources(
    device: &Device,
    resources: VulkanaliaSceneTransferFrameResources,
) {
    destroy_partial_scene_transfer_frame_resources(
        device,
        resources.command_pool,
        resources.image_available,
        resources.render_finished,
        resources.in_flight,
    );
}

fn destroy_partial_scene_transfer_frame_resources(
    device: &Device,
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
    }
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
    retain_cpu_topology: bool,
) -> Result<VulkanaliaSceneSolidQuadGeometryResources, String> {
    let vertex_buffer_count = vertex_buffer_count.max(1);
    let mut vertex_buffers = Vec::with_capacity(vertex_buffer_count);
    for vertex_buffer_index in 0..vertex_buffer_count {
        match create_scene_uploaded_buffer(
            device,
            memory_properties,
            &payload.vertex_bytes,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            retain_cpu_topology,
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
        false,
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

    let vertex_buffer_bytes = payload.vertex_bytes.len() as u64;
    let index_buffer_bytes = payload.index_bytes.len() as u64;
    let draw_step_count = payload.draw_steps.len().min(u32::MAX as usize) as u32;
    let draw_steps = payload.draw_steps;
    let indices = if retain_cpu_topology {
        payload.indices
    } else {
        Vec::new()
    };
    Ok(VulkanaliaSceneSolidQuadGeometryResources {
        vertex_buffers,
        index_buffer: index.buffer,
        index_memory: index.memory,
        draw_steps,
        indices,
        snapshot: NativeVulkanVulkanaliaSceneSolidQuadGeometrySnapshot {
            source_label: payload.source_label,
            vertex_count: payload.vertex_count,
            vertex_buffer_bytes,
            index_buffer_bytes,
            index_count: payload.index_count,
            quad_count: payload.quad_count,
            draw_step_count,
            vertex_stride_bytes: SCENE_FULL_SOLID_QUAD_VERTEX_STRIDE_BYTES,
            selected_vertex_memory_type_index,
            selected_index_memory_type_index: index.memory_type.index,
            vertex_memory_property_flags,
            index_memory_property_flags: memory_property_flag_labels(
                index.memory_type.property_flags_bits,
            ),
            upload_model: if vertex_buffer_count > 1 {
                "persistently mapped per-frame host-visible solid-quad vertex buffers reused by frame slot"
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

fn update_scene_solid_quad_geometry_input_for_time(
    device: &Device,
    geometry: &VulkanaliaSceneSolidQuadGeometryResources,
    frame_slot: usize,
    mut input: NativeVulkanVulkanaliaSceneSolidQuadGeometryInput,
    extent: vk::Extent2D,
    scene_size: Option<SceneSize>,
    scene_fit: FitMode,
) -> Result<vk::Buffer, String> {
    if geometry.vertex_buffers.is_empty() {
        return Err("scene solid-quad geometry has no vertex buffers".to_owned());
    }
    if let Some(transform) = scene_viewport_transform(scene_size, scene_fit, extent) {
        scene_solid_quad_apply_viewport(&mut input, transform);
    }
    if !input.indices.is_empty() && input.indices != geometry.indices {
        return Err("scene dynamic solid-quad geometry changed index topology".to_owned());
    }
    if !input.draw_steps.is_empty() && input.draw_steps != geometry.draw_steps {
        return Err("scene dynamic solid-quad geometry changed draw step topology".to_owned());
    }
    let expected_bytes = geometry.snapshot.vertex_buffer_bytes as usize;
    let vertex_bytes = input
        .vertices
        .len()
        .checked_mul(SCENE_FULL_SOLID_QUAD_VERTEX_STRIDE_BYTES as usize)
        .ok_or_else(|| "scene dynamic solid-quad vertex bytes overflow".to_owned())?;
    if vertex_bytes != expected_bytes {
        return Err(format!(
            "scene dynamic solid-quad vertex bytes {} did not match retained buffer bytes {}",
            vertex_bytes, expected_bytes
        ));
    }
    let vertex = geometry
        .vertex_buffers
        .get(frame_slot % geometry.vertex_buffers.len())
        .expect("scene solid-quad vertex buffer checked non-empty");
    write_scene_solid_quad_vertices_to_uploaded_buffer(
        device,
        vertex,
        &input.vertices,
        "dynamic scene solid-quad vertex",
    )?;
    Ok(vertex.buffer)
}

fn create_scene_sampled_image_geometry_resources(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    payload: VulkanaliaSceneSampledImageGeometryPayload,
    frame_resource_count: usize,
    retain_dynamic_topology: bool,
) -> Result<VulkanaliaSceneSampledImageGeometryResources, String> {
    let animated_geometry = scene_sampled_image_draw_steps_are_animated(&payload.draw_steps);
    let vertex_buffer_count = scene_sampled_image_vertex_buffer_count(
        &payload.draw_steps,
        frame_resource_count,
        retain_dynamic_topology,
    );
    let keep_vertex_buffers_mapped = animated_geometry || retain_dynamic_topology;
    let mut vertex_buffers = Vec::with_capacity(vertex_buffer_count);
    for vertex_buffer_index in 0..vertex_buffer_count {
        match create_scene_uploaded_buffer(
            device,
            memory_properties,
            &payload.vertex_bytes,
            vk::BufferUsageFlags::VERTEX_BUFFER,
            keep_vertex_buffers_mapped,
            if animated_geometry {
                "sampled-image per-frame vertex"
            } else if retain_dynamic_topology {
                "sampled-image dynamic per-frame vertex"
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
        false,
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

    let vertex_buffer_bytes = payload.vertex_bytes.len() as u64;
    let index_buffer_bytes = payload.index_bytes.len() as u64;
    let draw_step_count = payload.draw_steps.len().min(u32::MAX as usize) as u32;
    let animated_uv_steps = scene_sampled_image_animated_uv_steps(
        &payload.vertices,
        &payload.indices,
        &payload.draw_steps,
    )?;
    let draw_steps = payload.draw_steps;
    let indices = if retain_dynamic_topology {
        payload.indices
    } else {
        Vec::new()
    };
    let sources = if retain_dynamic_topology {
        payload.sources
    } else {
        Vec::new()
    };
    Ok(VulkanaliaSceneSampledImageGeometryResources {
        vertex_buffers,
        index_buffer: index.buffer,
        index_memory: index.memory,
        draw_steps,
        animated_uv_steps,
        indices,
        sources,
        snapshot: NativeVulkanVulkanaliaSceneSampledImageGeometrySnapshot {
            source_label: payload.source_label,
            vertex_count: payload.vertex_count,
            vertex_buffer_bytes,
            vertex_buffer_count: vertex_buffer_count.min(u32::MAX as usize) as u32,
            index_buffer_bytes,
            index_count: payload.index_count,
            quad_count: payload.quad_count,
            source_count: payload.source_count,
            draw_step_count,
            vertex_stride_bytes: SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES,
            selected_vertex_memory_type_index,
            selected_index_memory_type_index: index.memory_type.index,
            vertex_memory_property_flags,
            index_memory_property_flags: memory_property_flag_labels(
                index.memory_type.property_flags_bits,
            ),
            upload_model: if animated_geometry || retain_dynamic_topology {
                "persistently mapped per-frame host-visible sampled-image vertex buffers reused by frame slot"
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
    write_scene_uploaded_buffer_with(device, buffer, bytes.len(), label, |dst| {
        unsafe {
            ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len());
        }
        Ok(())
    })
}

fn write_scene_uploaded_buffer_with(
    device: &Device,
    buffer: &VulkanaliaSceneUploadedBuffer,
    byte_len: usize,
    label: &'static str,
    write: impl FnOnce(*mut u8) -> Result<(), String>,
) -> Result<(), String> {
    if byte_len as u64 > buffer.mapped_size.max(buffer.memory_size) {
        return Err(format!(
            "scene {label} write {} bytes exceeds buffer mapped size {}",
            byte_len,
            buffer.mapped_size.max(buffer.memory_size)
        ));
    }
    if let Some(mapped_ptr) = buffer.mapped_ptr {
        write(mapped_ptr.cast::<u8>())?;
        let host_coherent = buffer.memory_type.property_flags_bits
            & vk::MemoryPropertyFlags::HOST_COHERENT.bits()
            == vk::MemoryPropertyFlags::HOST_COHERENT.bits();
        if !host_coherent {
            let range = vk::MappedMemoryRange::builder()
                .memory(buffer.memory)
                .offset(0)
                .size(vk::WHOLE_SIZE)
                .build();
            unsafe {
                device.flush_mapped_memory_ranges(&[range]).map_err(|err| {
                    format!("vkFlushMappedMemoryRanges(vulkanalia {label}): {err:?}")
                })?;
            }
        }
        return Ok(());
    }

    let map = native_vulkan_vulkanalia_map_memory2(
        device,
        buffer.memory,
        0,
        byte_len as u64,
        vk::MemoryMapFlags::empty(),
        label,
    )?;
    if let Err(err) = write(map.cast::<u8>()) {
        let _ = native_vulkan_vulkanalia_unmap_memory2(device, buffer.memory, label);
        return Err(err);
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

fn write_scene_solid_quad_vertices_to_uploaded_buffer(
    device: &Device,
    buffer: &VulkanaliaSceneUploadedBuffer,
    vertices: &[NativeVulkanVulkanaliaSceneSolidQuadVertex],
    label: &'static str,
) -> Result<(), String> {
    let byte_len = vertices
        .len()
        .checked_mul(SCENE_FULL_SOLID_QUAD_VERTEX_STRIDE_BYTES as usize)
        .ok_or_else(|| format!("scene {label} vertex byte length overflows"))?;
    write_scene_uploaded_buffer_with(device, buffer, byte_len, label, |dst| {
        let mut offset = 0usize;
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
                write_scene_f32_to_mapped(dst, &mut offset, value);
            }
        }
        Ok(())
    })
}

fn write_scene_sampled_image_vertices_to_uploaded_buffer(
    device: &Device,
    buffer: &VulkanaliaSceneUploadedBuffer,
    vertices: &[NativeVulkanVulkanaliaSceneSampledImageVertex],
    animated_uv_steps: &[SceneSampledImageAnimatedUvStep],
    elapsed_ms: u64,
    viewport_transform: Option<SceneViewportTransform>,
    label: &'static str,
) -> Result<(), String> {
    scene_sampled_image_validate_animated_uv_steps(vertices.len(), animated_uv_steps)?;
    let byte_len = vertices
        .len()
        .checked_mul(SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES as usize)
        .ok_or_else(|| format!("scene {label} vertex byte length overflows"))?;
    write_scene_uploaded_buffer_with(device, buffer, byte_len, label, |dst| {
        let mut offset = 0usize;
        for (index, vertex) in vertices.iter().enumerate() {
            let uv =
                scene_sampled_image_uv_for_time(vertex.uv, index, animated_uv_steps, elapsed_ms);
            let position = viewport_transform
                .map(|transform| scene_viewport_transform_position(vertex.position, transform))
                .unwrap_or(vertex.position);
            if !position
                .into_iter()
                .chain(uv)
                .chain([vertex.opacity])
                .chain(vertex.tint)
                .all(f32::is_finite)
            {
                return Err(format!(
                    "scene sampled-image vertex {index} contains a non-finite value"
                ));
            }
            for value in position
                .into_iter()
                .chain(uv)
                .chain([vertex.opacity])
                .chain(vertex.tint)
            {
                write_scene_f32_to_mapped(dst, &mut offset, value);
            }
        }
        Ok(())
    })
}

fn write_scene_sampled_image_animated_uvs_to_uploaded_buffer(
    device: &Device,
    buffer: &VulkanaliaSceneUploadedBuffer,
    vertex_count: usize,
    animated_uv_steps: &[SceneSampledImageAnimatedUvStep],
    elapsed_ms: u64,
    label: &'static str,
) -> Result<(), String> {
    let byte_len = vertex_count
        .checked_mul(SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES as usize)
        .ok_or_else(|| format!("scene {label} vertex byte length overflows"))?;
    write_scene_uploaded_buffer_with(device, buffer, byte_len, label, |dst| {
        let bytes = unsafe { std::slice::from_raw_parts_mut(dst, byte_len) };
        patch_scene_sampled_image_animated_uvs_in_bytes(
            bytes,
            vertex_count,
            animated_uv_steps,
            elapsed_ms,
        )
    })
}

fn patch_scene_sampled_image_animated_uvs_in_bytes(
    bytes: &mut [u8],
    vertex_count: usize,
    animated_uv_steps: &[SceneSampledImageAnimatedUvStep],
    elapsed_ms: u64,
) -> Result<(), String> {
    scene_sampled_image_validate_animated_uv_steps(vertex_count, animated_uv_steps)?;
    let expected_bytes = vertex_count
        .checked_mul(SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES as usize)
        .ok_or_else(|| "scene sampled-image animated vertex byte length overflows".to_owned())?;
    if bytes.len() < expected_bytes {
        return Err(format!(
            "scene sampled-image animated UV patch buffer has {} bytes, expected at least {}",
            bytes.len(),
            expected_bytes
        ));
    }
    for animated_step in animated_uv_steps {
        for (uv_index, vertex_index) in animated_step.vertex_indices.iter().enumerate() {
            let vertex_offset = (*vertex_index as usize)
                .checked_mul(SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES as usize)
                .and_then(|offset| {
                    offset.checked_add(SCENE_FULL_SAMPLED_IMAGE_VERTEX_UV_OFFSET_BYTES)
                })
                .ok_or_else(|| {
                    "scene sampled-image animated UV patch offset overflows".to_owned()
                })?;
            let uv_end = vertex_offset
                .checked_add(SCENE_FULL_SAMPLED_IMAGE_VERTEX_UV_BYTES)
                .ok_or_else(|| {
                    "scene sampled-image animated UV patch end offset overflows".to_owned()
                })?;
            if uv_end > bytes.len() {
                return Err(format!(
                    "scene sampled-image animated UV patch range {vertex_offset}..{uv_end} exceeds buffer bytes {}",
                    bytes.len()
                ));
            }
            let uv = scene_sampled_image_animated_uv_at_elapsed(
                animated_step.base_uvs[uv_index],
                *animated_step,
                elapsed_ms,
            );
            if !uv.into_iter().all(f32::is_finite) {
                return Err(format!(
                    "scene sampled-image animated UV for vertex {vertex_index} contains a non-finite value"
                ));
            }
            let mut offset = vertex_offset;
            for value in uv {
                write_scene_f32_to_mapped(bytes.as_mut_ptr(), &mut offset, value);
            }
        }
    }
    Ok(())
}

fn write_scene_f32_to_mapped(dst: *mut u8, offset: &mut usize, value: f32) {
    let bytes = value.to_ne_bytes();
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), dst.add(*offset), bytes.len());
    }
    *offset += bytes.len();
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
    if let Some(payload) = dynamic_payload {
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
        let expected_bytes = geometry.snapshot.vertex_buffer_bytes as usize;
        if payload.vertex_bytes.len() != expected_bytes {
            return Err(format!(
                "scene sampled-image dynamic vertex bytes {} did not match retained buffer bytes {}",
                payload.vertex_bytes.len(),
                expected_bytes
            ));
        }
        write_scene_uploaded_buffer(
            device,
            vertex,
            &payload.vertex_bytes,
            "dynamic scene sampled-image vertex",
        )?;
        return Ok(vertex.buffer);
    } else {
        let expected_bytes = geometry.snapshot.vertex_buffer_bytes as usize;
        let vertex_count = geometry.snapshot.vertex_count as usize;
        let vertex_bytes = vertex_count
            .checked_mul(SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES as usize)
            .ok_or_else(|| "scene sampled-image animated vertex bytes overflow".to_owned())?;
        if vertex_bytes != expected_bytes {
            return Err(format!(
                "scene sampled-image animated vertex bytes {} did not match retained buffer bytes {}",
                vertex_bytes, expected_bytes
            ));
        }
        write_scene_sampled_image_animated_uvs_to_uploaded_buffer(
            device,
            vertex,
            vertex_count,
            &geometry.animated_uv_steps,
            elapsed_ms,
            "animated scene sampled-image vertex",
        )?;
    }
    Ok(vertex.buffer)
}

fn update_scene_sampled_image_geometry_input_for_time(
    device: &Device,
    geometry: &VulkanaliaSceneSampledImageGeometryResources,
    frame_slot: usize,
    mut input: NativeVulkanVulkanaliaSceneSampledImageGeometryInput,
    extent: vk::Extent2D,
    scene_size: Option<SceneSize>,
    scene_fit: FitMode,
) -> Result<vk::Buffer, String> {
    if geometry.vertex_buffers.is_empty() {
        return Err("scene sampled-image geometry has no vertex buffers".to_owned());
    }
    let viewport_transform = scene_viewport_transform(scene_size, scene_fit, extent);
    if !input.indices.is_empty() && input.indices != geometry.indices {
        return Err("scene dynamic sampled-image geometry changed index topology".to_owned());
    }
    if !input.sources.is_empty() && input.sources != geometry.sources {
        return Err("scene dynamic sampled-image geometry changed sampled sources".to_owned());
    }
    if !input.draw_steps.is_empty()
        && !scene_sampled_image_draw_step_topology_matches(&input.draw_steps, &geometry.draw_steps)
    {
        return Err("scene dynamic sampled-image geometry changed draw step topology".to_owned());
    }
    if !input.sources.is_empty() || !input.draw_steps.is_empty() {
        let source_count = if input.sources.is_empty() {
            geometry.sources.len().max(1)
        } else {
            input.sources.len().max(1)
        };
        let draw_steps = if input.draw_steps.is_empty() {
            geometry.draw_steps.as_slice()
        } else {
            input.draw_steps.as_slice()
        };
        for (step_index, step) in draw_steps.iter().enumerate() {
            if step.resource_index as usize >= source_count {
                return Err(format!(
                    "scene dynamic sampled-image draw step {step_index} resource index {} exceeds source count {}",
                    step.resource_index, source_count
                ));
            }
        }
    }
    let expected_bytes = geometry.snapshot.vertex_buffer_bytes as usize;
    let vertex_bytes = input
        .vertices
        .len()
        .checked_mul(SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES as usize)
        .ok_or_else(|| "scene sampled-image dynamic vertex bytes overflow".to_owned())?;
    if vertex_bytes != expected_bytes {
        return Err(format!(
            "scene sampled-image dynamic vertex bytes {} did not match retained buffer bytes {}",
            vertex_bytes, expected_bytes
        ));
    }
    let vertex = geometry
        .vertex_buffers
        .get(frame_slot % geometry.vertex_buffers.len())
        .expect("scene sampled-image vertex buffer checked non-empty");
    write_scene_sampled_image_vertices_to_uploaded_buffer(
        device,
        vertex,
        &input.vertices,
        &[],
        0,
        viewport_transform,
        "dynamic scene sampled-image vertex",
    )?;
    native_vulkan_vulkanalia_recycle_scene_sampled_image_vertex_vec(std::mem::take(
        &mut input.vertices,
    ));
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
    for animated_step in
        scene_sampled_image_animated_uv_steps(vertices, geometry_indices, draw_steps)?
    {
        let vertex_count = vertices.len();
        for (uv_index, vertex_index) in animated_step.vertex_indices.iter().enumerate() {
            let vertex = vertices.get_mut(*vertex_index as usize).ok_or_else(|| {
                format!(
                    "scene sampled-image animated vertex index {vertex_index} exceeds vertex count {}",
                    vertex_count
                )
            })?;
            vertex.uv = scene_sampled_image_animated_uv_at_elapsed(
                animated_step.base_uvs[uv_index],
                animated_step,
                elapsed_ms,
            );
        }
    }
    Ok(())
}

fn scene_sampled_image_animated_uv_steps(
    vertices: &[NativeVulkanVulkanaliaSceneSampledImageVertex],
    geometry_indices: &[u32],
    draw_steps: &[NativeVulkanVulkanaliaSceneSampledImageDrawStep],
) -> Result<Vec<SceneSampledImageAnimatedUvStep>, String> {
    let mut animated_steps = Vec::new();
    for step in draw_steps {
        let Some(texture_region) = step.texture_region else {
            continue;
        };
        if !scene_texture_region_is_animated(Some(texture_region)) {
            continue;
        }
        let vertex_indices = scene_sampled_image_draw_step_unique_vertices(geometry_indices, step)?;
        let mut base_uvs = [[0.0f32; 2]; 4];
        for (uv_index, vertex_index) in vertex_indices.iter().enumerate() {
            let vertex = vertices.get(*vertex_index as usize).ok_or_else(|| {
                format!(
                    "scene sampled-image animated vertex index {vertex_index} exceeds vertex count {}",
                    vertices.len()
                )
            })?;
            base_uvs[uv_index] = vertex.uv;
        }
        animated_steps.push(SceneSampledImageAnimatedUvStep {
            vertex_indices,
            base_uvs,
            texture_region,
        });
    }
    Ok(animated_steps)
}

fn scene_sampled_image_draw_step_unique_vertices(
    geometry_indices: &[u32],
    step: &NativeVulkanVulkanaliaSceneSampledImageDrawStep,
) -> Result<[u32; 4], String> {
    let end_index = step
        .first_index
        .checked_add(step.index_count)
        .ok_or_else(|| "scene sampled-image animated index range overflows".to_owned())?;
    let indices = geometry_indices
        .get(step.first_index as usize..end_index as usize)
        .ok_or_else(|| {
            "scene sampled-image animated index range exceeds geometry indices".to_owned()
        })?;
    let mut unique_vertices = [0u32; 4];
    let mut unique_count = 0usize;
    for index in indices {
        if unique_vertices[..unique_count].contains(index) {
            continue;
        }
        if unique_count == unique_vertices.len() {
            return Err(format!(
                "scene sampled-image animated draw step for layer {} expected 4 unique vertices, got more than 4",
                step.layer_index
            ));
        }
        unique_vertices[unique_count] = *index;
        unique_count += 1;
    }
    if unique_count != unique_vertices.len() {
        return Err(format!(
            "scene sampled-image animated draw step for layer {} expected 4 unique vertices, got {}",
            step.layer_index, unique_count
        ));
    }
    Ok(unique_vertices)
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
    dynamic_geometry: bool,
) -> usize {
    if dynamic_geometry || scene_sampled_image_draw_steps_are_animated(draw_steps) {
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
    keep_mapped: bool,
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

        let mapped_size = if keep_mapped {
            memory_requirements.size
        } else {
            payload.len() as u64
        };
        let map = match native_vulkan_vulkanalia_map_memory2(
            device,
            memory,
            0,
            mapped_size,
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
        let mapped_ptr = if keep_mapped {
            Some(map)
        } else {
            native_vulkan_vulkanalia_unmap_memory2(device, memory, label)?;
            None
        };

        Ok(VulkanaliaSceneUploadedBuffer {
            buffer,
            memory,
            memory_type,
            memory_size: memory_requirements.size,
            mapped_ptr,
            mapped_size,
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
    if buffer.mapped_ptr.is_some() {
        let _ = native_vulkan_vulkanalia_unmap_memory2(device, buffer.memory, "scene upload");
    }
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
    input: Option<NativeVulkanVulkanaliaSceneSolidQuadGeometryInput>,
    extent: vk::Extent2D,
    color: NativeVulkanClearColor,
    scene_size: Option<SceneSize>,
    scene_fit: FitMode,
) -> Result<VulkanaliaSceneSolidQuadGeometryPayload, String> {
    let mut viewport_transformed = false;
    let mut input = if let Some(mut input) = input {
        if let Some(transform) = scene_viewport_transform(scene_size, scene_fit, extent) {
            scene_solid_quad_apply_viewport(&mut input, transform);
            viewport_transformed = true;
        }
        input
    } else {
        scene_solid_quad_full_extent_geometry_input(extent, color)
    };
    if viewport_transformed {
        input.source_label = format!("{}+scene-viewport-fit", input.source_label);
    }
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
    input: NativeVulkanVulkanaliaSceneSolidQuadGeometryInput,
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
    let vertex_count = input.vertices.len() as u32;
    let index_count = input.indices.len() as u32;
    let quad_count = (input.indices.len() / SCENE_FULL_SOLID_QUAD_INDEX_COUNT as usize) as u32;
    Ok(VulkanaliaSceneSolidQuadGeometryPayload {
        indices: input.indices,
        vertex_bytes,
        index_bytes,
        vertex_count,
        index_count,
        quad_count,
        draw_steps: input.draw_steps,
        source_label: input.source_label,
    })
}

fn scene_sampled_image_geometry_payload(
    input: Option<NativeVulkanVulkanaliaSceneSampledImageGeometryInput>,
    extent: vk::Extent2D,
    fit: Option<FitMode>,
    source_extent: vk::Extent2D,
    scene_size: Option<SceneSize>,
    scene_fit: FitMode,
) -> Result<VulkanaliaSceneSampledImageGeometryPayload, String> {
    let mut viewport_transformed = false;
    let mut input = if let Some(mut input) = input {
        if let Some(transform) = scene_viewport_transform(scene_size, scene_fit, extent) {
            scene_sampled_image_apply_viewport(&mut input, transform);
            viewport_transformed = true;
        }
        input
    } else if let Some(fit) = fit {
        scene_sampled_image_fit_geometry_input(extent, source_extent, fit)?
    } else {
        scene_sampled_image_full_extent_geometry_input(extent)
    };
    if viewport_transformed {
        input.source_label = format!("{}+scene-viewport-fit", input.source_label);
    }
    scene_sampled_image_geometry_payload_from_input(input)
}

fn scene_video_layer_geometry_payload(
    input: NativeVulkanVulkanaliaSceneVideoLayerGeometryInput,
    extent: vk::Extent2D,
    scene_size: Option<SceneSize>,
    scene_fit: FitMode,
) -> Result<VulkanaliaSceneSampledImageGeometryPayload, String> {
    let mut sampled_input = NativeVulkanVulkanaliaSceneSampledImageGeometryInput::new_batched(
        input.vertices,
        input.indices,
        input.sources,
        input
            .draw_steps
            .into_iter()
            .map(|step| NativeVulkanVulkanaliaSceneSampledImageDrawStep {
                layer_index: step.layer_index,
                resource_index: step.resource_index,
                first_index: step.first_index,
                index_count: step.index_count,
                blend_mode: SceneBlendMode::Alpha,
                fit: step.fit,
                texture_region: None,
            })
            .collect(),
        input.source_label,
    );
    if let Some(transform) = scene_viewport_transform(scene_size, scene_fit, extent) {
        scene_sampled_image_apply_viewport(&mut sampled_input, transform);
        sampled_input.source_label = format!("{}+scene-viewport-fit", sampled_input.source_label);
    }
    scene_sampled_image_geometry_payload_from_input(sampled_input)
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

fn scene_solid_quad_apply_viewport(
    geometry: &mut NativeVulkanVulkanaliaSceneSolidQuadGeometryInput,
    transform: SceneViewportTransform,
) {
    for vertex in &mut geometry.vertices {
        vertex.position = scene_viewport_transform_position(vertex.position, transform);
    }
}

fn scene_sampled_image_apply_viewport(
    geometry: &mut NativeVulkanVulkanaliaSceneSampledImageGeometryInput,
    transform: SceneViewportTransform,
) {
    for vertex in &mut geometry.vertices {
        vertex.position = scene_viewport_transform_position(vertex.position, transform);
    }
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
    scene_sampled_image_draw_commands_for_count(draw_steps, sampled_images.len())
}

fn scene_sampled_image_draw_commands_for_count(
    draw_steps: &[NativeVulkanVulkanaliaSceneSampledImageDrawStep],
    sampled_image_count: usize,
) -> Result<Vec<VulkanaliaSceneSampledImageDrawCommand>, String> {
    let mut draw_commands = Vec::with_capacity(draw_steps.len());
    for (step_index, step) in draw_steps.iter().enumerate() {
        if step.resource_index as usize >= sampled_image_count {
            return Err(format!(
                "scene sampled-image draw step {step_index} resource index {} exceeds sampled image count {}",
                step.resource_index, sampled_image_count
            ));
        }
        if step.index_count == 0 {
            return Err(format!(
                "scene sampled-image draw step {step_index} requires at least one index"
            ));
        }
        let descriptor_binding = VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap {
            resource_index: step.resource_index,
        };
        let command = VulkanaliaSceneSampledImageDrawCommand {
            layer_index: step.layer_index,
            last_layer_index: step.layer_index,
            blend_mode: step.blend_mode,
            descriptor_binding,
            first_index: step.first_index,
            index_count: step.index_count,
        };
        if let Some(previous) = draw_commands.last_mut()
            && scene_sampled_image_draw_commands_can_merge(previous, &command)
        {
            previous.index_count = previous.index_count.saturating_add(command.index_count);
            previous.last_layer_index = command.last_layer_index;
            continue;
        }
        draw_commands.push(command);
    }
    Ok(draw_commands)
}

fn scene_sampled_image_draw_commands_can_merge(
    previous: &VulkanaliaSceneSampledImageDrawCommand,
    next: &VulkanaliaSceneSampledImageDrawCommand,
) -> bool {
    previous.last_layer_index.saturating_add(1) == next.layer_index
        && previous.blend_mode == next.blend_mode
        && previous.descriptor_binding == next.descriptor_binding
        && previous
            .first_index
            .checked_add(previous.index_count)
            .is_some_and(|next_first_index| next_first_index == next.first_index)
}

fn scene_video_layer_draw_commands(
    draw_steps: &[NativeVulkanVulkanaliaSceneSampledImageDrawStep],
) -> Result<Vec<VulkanaliaSceneVideoLayerDrawCommand>, String> {
    if draw_steps.is_empty() {
        return Err("scene video layer draw command list requires at least one step".to_owned());
    }
    let mut draw_commands = Vec::with_capacity(draw_steps.len());
    for (step_index, step) in draw_steps.iter().enumerate() {
        if step.index_count == 0 {
            return Err(format!(
                "scene video layer draw step {step_index} requires at least one index"
            ));
        }
        draw_commands.push(VulkanaliaSceneVideoLayerDrawCommand {
            layer_index: step.layer_index,
            resource_index: step.resource_index,
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
        let command = VulkanaliaSceneSolidQuadDrawCommand {
            layer_index: step.layer_index,
            last_layer_index: step.layer_index,
            blend_mode: step.blend_mode,
            first_index: step.first_index,
            index_count: step.index_count,
        };
        if let Some(previous) = draw_commands.last_mut()
            && scene_solid_quad_draw_commands_can_merge(previous, &command)
        {
            previous.index_count = previous.index_count.saturating_add(command.index_count);
            previous.last_layer_index = command.last_layer_index;
            continue;
        }
        draw_commands.push(command);
    }
    Ok(draw_commands)
}

fn scene_solid_quad_draw_commands_can_merge(
    previous: &VulkanaliaSceneSolidQuadDrawCommand,
    next: &VulkanaliaSceneSolidQuadDrawCommand,
) -> bool {
    previous.last_layer_index.saturating_add(1) == next.layer_index
        && previous.blend_mode == next.blend_mode
        && previous
            .first_index
            .checked_add(previous.index_count)
            .is_some_and(|next_first_index| next_first_index == next.first_index)
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
    input: NativeVulkanVulkanaliaSceneSampledImageGeometryInput,
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
    let vertex_count = input.vertices.len() as u32;
    let index_count = input.indices.len() as u32;
    let quad_count = (input.indices.len() / SCENE_FULL_SAMPLED_IMAGE_INDEX_COUNT as usize) as u32;
    Ok(VulkanaliaSceneSampledImageGeometryPayload {
        vertices: input.vertices,
        indices: input.indices,
        sources: input.sources,
        vertex_bytes,
        index_bytes,
        vertex_count,
        index_count,
        quad_count,
        source_count: source_count.min(u32::MAX as usize) as u32,
        draw_steps: input.draw_steps,
        source_label: input.source_label,
    })
}

fn scene_solid_quad_vertex_bytes(
    vertices: &[NativeVulkanVulkanaliaSceneSolidQuadVertex],
) -> Result<SceneGeometryByteBuffer, String> {
    let mut bytes = SceneGeometryByteBuffer::with_capacity(
        vertices.len() * SCENE_FULL_SOLID_QUAD_VERTEX_STRIDE_BYTES as usize,
    );
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
) -> Result<SceneGeometryByteBuffer, String> {
    scene_sampled_image_vertex_bytes_for_time(vertices, &[], 0)
}

fn scene_sampled_image_vertex_bytes_for_time(
    vertices: &[NativeVulkanVulkanaliaSceneSampledImageVertex],
    animated_uv_steps: &[SceneSampledImageAnimatedUvStep],
    elapsed_ms: u64,
) -> Result<SceneGeometryByteBuffer, String> {
    scene_sampled_image_validate_animated_uv_steps(vertices.len(), animated_uv_steps)?;
    let mut bytes = SceneGeometryByteBuffer::with_capacity(
        vertices.len() * SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES as usize,
    );
    for (index, vertex) in vertices.iter().enumerate() {
        if !vertex
            .position
            .into_iter()
            .chain(vertex.uv)
            .chain([vertex.opacity])
            .chain(vertex.tint)
            .all(f32::is_finite)
        {
            return Err(format!(
                "scene sampled-image vertex {index} contains a non-finite value"
            ));
        }
        let uv = scene_sampled_image_uv_for_time(vertex.uv, index, animated_uv_steps, elapsed_ms);
        for value in vertex
            .position
            .into_iter()
            .chain(uv)
            .chain([vertex.opacity])
            .chain(vertex.tint)
        {
            bytes.extend_from_slice(&value.to_ne_bytes());
        }
    }
    Ok(bytes)
}

fn scene_sampled_image_validate_animated_uv_steps(
    vertex_count: usize,
    animated_uv_steps: &[SceneSampledImageAnimatedUvStep],
) -> Result<(), String> {
    for animated_step in animated_uv_steps {
        for vertex_index in animated_step.vertex_indices {
            if vertex_index as usize >= vertex_count {
                return Err(format!(
                    "scene sampled-image animated vertex index {vertex_index} exceeds vertex count {vertex_count}"
                ));
            }
        }
    }
    Ok(())
}

fn scene_sampled_image_uv_for_time(
    base_uv: [f32; 2],
    vertex_index: usize,
    animated_uv_steps: &[SceneSampledImageAnimatedUvStep],
    elapsed_ms: u64,
) -> [f32; 2] {
    animated_uv_steps
        .iter()
        .find(|step| {
            step.vertex_indices
                .iter()
                .any(|animated_vertex_index| *animated_vertex_index as usize == vertex_index)
        })
        .map(|step| scene_sampled_image_animated_uv_at_elapsed(base_uv, *step, elapsed_ms))
        .unwrap_or(base_uv)
}

fn scene_sampled_image_animated_uv_at_elapsed(
    base_uv: [f32; 2],
    animated_step: SceneSampledImageAnimatedUvStep,
    elapsed_ms: u64,
) -> [f32; 2] {
    let base_region = animated_step.texture_region;
    let elapsed_region = scene_texture_region_at_elapsed(base_region, elapsed_ms);
    [
        scene_texture_region_remap_axis(
            base_uv[0],
            base_region.u_min,
            base_region.u_max,
            elapsed_region.u_min,
            elapsed_region.u_max,
        ),
        scene_texture_region_remap_axis(
            base_uv[1],
            base_region.v_min,
            base_region.v_max,
            elapsed_region.v_min,
            elapsed_region.v_max,
        ),
    ]
}

fn scene_texture_region_remap_axis(
    value: f32,
    base_min: f64,
    base_max: f64,
    elapsed_min: f64,
    elapsed_max: f64,
) -> f32 {
    let base_span = base_max - base_min;
    if !base_span.is_finite() || base_span <= f64::EPSILON {
        return elapsed_min as f32;
    }
    let t = ((f64::from(value) - base_min) / base_span).clamp(0.0, 1.0);
    (elapsed_min + (elapsed_max - elapsed_min) * t) as f32
}

fn scene_solid_quad_index_bytes(
    indices: &[u32],
    vertex_count: usize,
) -> Result<SceneGeometryByteBuffer, String> {
    scene_geometry_index_bytes(indices, vertex_count, "solid quad")
}

fn scene_geometry_index_bytes(
    indices: &[u32],
    vertex_count: usize,
    label: &'static str,
) -> Result<SceneGeometryByteBuffer, String> {
    let max_index = (vertex_count - 1) as u32;
    let mut bytes = SceneGeometryByteBuffer::with_capacity(indices.len() * 4);
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

fn submit_scene_transfer_command_buffer2(
    device: &Device,
    queue: vk::Queue,
    command_buffer: vk::CommandBuffer,
    image_available: vk::Semaphore,
    render_finished: vk::Semaphore,
    fence: vk::Fence,
) -> Result<(), String> {
    let wait = vk::SemaphoreSubmitInfo::builder()
        .semaphore(image_available)
        .stage_mask(vk::PipelineStageFlags2::ALL_TRANSFER)
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
            .map_err(|err| format!("vkQueueSubmit2(vulkanalia scene transfer present): {err:?}"))?;
    }

    Ok(())
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
    fn scene_present_id_telemetry_retains_no_frame_ids_by_default() {
        let mut telemetry = ScenePresentIdTelemetry::new();
        for present_id in 1..=128 {
            telemetry.push(Some(present_id));
        }

        let (head, tail) = telemetry.into_parts();

        assert_eq!(SCENE_PRESENT_ID_TELEMETRY_RETAINED_FRAMES, 0);
        assert!(head.is_empty());
        assert!(tail.is_empty());
    }

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
    fn solid_quad_draw_commands_merge_contiguous_layer_ranges() {
        let commands = scene_solid_quad_draw_commands(&[
            NativeVulkanVulkanaliaSceneSolidQuadDrawStep {
                layer_index: 4,
                first_index: 0,
                index_count: 6,
                blend_mode: SceneBlendMode::Alpha,
            },
            NativeVulkanVulkanaliaSceneSolidQuadDrawStep {
                layer_index: 5,
                first_index: 6,
                index_count: 6,
                blend_mode: SceneBlendMode::Alpha,
            },
            NativeVulkanVulkanaliaSceneSolidQuadDrawStep {
                layer_index: 6,
                first_index: 12,
                index_count: 12,
                blend_mode: SceneBlendMode::Alpha,
            },
        ])
        .unwrap();

        assert_eq!(
            commands,
            vec![VulkanaliaSceneSolidQuadDrawCommand {
                layer_index: 4,
                last_layer_index: 6,
                blend_mode: SceneBlendMode::Alpha,
                first_index: 0,
                index_count: 24,
            }]
        );
    }

    #[test]
    fn solid_quad_draw_commands_keep_non_contiguous_layer_ranges_separate() {
        let commands = scene_solid_quad_draw_commands(&[
            NativeVulkanVulkanaliaSceneSolidQuadDrawStep {
                layer_index: 4,
                first_index: 0,
                index_count: 6,
                blend_mode: SceneBlendMode::Alpha,
            },
            NativeVulkanVulkanaliaSceneSolidQuadDrawStep {
                layer_index: 6,
                first_index: 6,
                index_count: 6,
                blend_mode: SceneBlendMode::Alpha,
            },
            NativeVulkanVulkanaliaSceneSolidQuadDrawStep {
                layer_index: 7,
                first_index: 24,
                index_count: 6,
                blend_mode: SceneBlendMode::Alpha,
            },
        ])
        .unwrap();

        assert_eq!(
            commands,
            vec![
                VulkanaliaSceneSolidQuadDrawCommand {
                    layer_index: 4,
                    last_layer_index: 4,
                    blend_mode: SceneBlendMode::Alpha,
                    first_index: 0,
                    index_count: 6,
                },
                VulkanaliaSceneSolidQuadDrawCommand {
                    layer_index: 6,
                    last_layer_index: 6,
                    blend_mode: SceneBlendMode::Alpha,
                    first_index: 6,
                    index_count: 6,
                },
                VulkanaliaSceneSolidQuadDrawCommand {
                    layer_index: 7,
                    last_layer_index: 7,
                    blend_mode: SceneBlendMode::Alpha,
                    first_index: 24,
                    index_count: 6,
                }
            ]
        );
    }

    #[test]
    fn solid_quad_draw_commands_keep_different_blend_modes_separate() {
        let commands = scene_solid_quad_draw_commands(&[
            NativeVulkanVulkanaliaSceneSolidQuadDrawStep {
                layer_index: 4,
                first_index: 0,
                index_count: 6,
                blend_mode: SceneBlendMode::Alpha,
            },
            NativeVulkanVulkanaliaSceneSolidQuadDrawStep {
                layer_index: 5,
                first_index: 6,
                index_count: 6,
                blend_mode: SceneBlendMode::Screen,
            },
        ])
        .unwrap();

        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].blend_mode, SceneBlendMode::Alpha);
        assert_eq!(commands[1].blend_mode, SceneBlendMode::Screen);
    }

    #[test]
    fn sampled_image_draw_commands_merge_same_resource_contiguous_ranges() {
        let commands = scene_sampled_image_draw_commands_for_count(
            &[
                NativeVulkanVulkanaliaSceneSampledImageDrawStep {
                    layer_index: 10,
                    resource_index: 0,
                    first_index: 0,
                    index_count: 6,
                    blend_mode: SceneBlendMode::Alpha,
                    fit: None,
                    texture_region: None,
                },
                NativeVulkanVulkanaliaSceneSampledImageDrawStep {
                    layer_index: 11,
                    resource_index: 0,
                    first_index: 6,
                    index_count: 12,
                    blend_mode: SceneBlendMode::Alpha,
                    fit: None,
                    texture_region: None,
                },
            ],
            1,
        )
        .unwrap();

        assert_eq!(
            commands,
            vec![VulkanaliaSceneSampledImageDrawCommand {
                layer_index: 10,
                last_layer_index: 11,
                blend_mode: SceneBlendMode::Alpha,
                descriptor_binding: VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap {
                    resource_index: 0,
                },
                first_index: 0,
                index_count: 18,
            }]
        );
    }

    #[test]
    fn sampled_image_draw_commands_keep_different_resources_separate() {
        let commands = scene_sampled_image_draw_commands_for_count(
            &[
                NativeVulkanVulkanaliaSceneSampledImageDrawStep {
                    layer_index: 10,
                    resource_index: 0,
                    first_index: 0,
                    index_count: 6,
                    blend_mode: SceneBlendMode::Alpha,
                    fit: None,
                    texture_region: None,
                },
                NativeVulkanVulkanaliaSceneSampledImageDrawStep {
                    layer_index: 11,
                    resource_index: 1,
                    first_index: 6,
                    index_count: 6,
                    blend_mode: SceneBlendMode::Alpha,
                    fit: None,
                    texture_region: None,
                },
            ],
            2,
        )
        .unwrap();

        assert_eq!(commands.len(), 2);
        assert_eq!(
            commands[0].descriptor_binding,
            VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap { resource_index: 0 }
        );
        assert_eq!(
            commands[1].descriptor_binding,
            VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap { resource_index: 1 }
        );
    }

    #[test]
    fn sampled_image_draw_commands_keep_different_blend_modes_separate() {
        let commands = scene_sampled_image_draw_commands_for_count(
            &[
                NativeVulkanVulkanaliaSceneSampledImageDrawStep {
                    layer_index: 10,
                    resource_index: 0,
                    first_index: 0,
                    index_count: 6,
                    blend_mode: SceneBlendMode::Alpha,
                    fit: None,
                    texture_region: None,
                },
                NativeVulkanVulkanaliaSceneSampledImageDrawStep {
                    layer_index: 11,
                    resource_index: 0,
                    first_index: 6,
                    index_count: 6,
                    blend_mode: SceneBlendMode::Max,
                    fit: None,
                    texture_region: None,
                },
            ],
            1,
        )
        .unwrap();

        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].blend_mode, SceneBlendMode::Alpha);
        assert_eq!(commands[1].blend_mode, SceneBlendMode::Max);
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

        let payload = scene_solid_quad_geometry_payload_from_input(input).unwrap();

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

        let err = scene_solid_quad_geometry_payload_from_input(input).unwrap_err();

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
        assert_eq!(payload.vertex_bytes.len(), 144);
        assert_eq!(payload.index_bytes.len(), 24);
        let floats = payload
            .vertex_bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_ne_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(
            &floats[0..9],
            &[0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0]
        );
        assert_eq!(
            &floats[27..36],
            &[0.0, 500.0, 0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]
        );
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
    fn static_transfer_present_only_accepts_plain_static_single_image() {
        assert!(scene_sampled_image_can_use_static_transfer_present(
            &static_transfer_test_options(Some(FitMode::Cover), None, None)
        ));
        assert!(!scene_sampled_image_can_use_static_transfer_present(
            &static_transfer_test_options(Some(FitMode::Tile), None, None)
        ));
        assert!(!scene_sampled_image_can_use_static_transfer_present(
            &static_transfer_test_options(
                Some(FitMode::Cover),
                Some(scene_sampled_image_full_extent_geometry_input(
                    vk::Extent2D {
                        width: 100,
                        height: 100,
                    },
                )),
                None,
            )
        ));
        assert!(!scene_sampled_image_can_use_static_transfer_present(
            &static_transfer_test_options(
                Some(FitMode::Cover),
                None,
                Some(Box::new(|_| {
                    Ok(scene_sampled_image_full_extent_geometry_input(
                        vk::Extent2D {
                            width: 100,
                            height: 100,
                        },
                    ))
                })),
            )
        ));
    }

    #[test]
    fn static_transfer_blit_region_preserves_fit_without_cpu_pixels() {
        let source = vk::Extent2D {
            width: 3840,
            height: 2160,
        };
        let target = vk::Extent2D {
            width: 1000,
            height: 500,
        };

        let contain = scene_static_transfer_blit_region(FitMode::Contain, source, target).unwrap();
        assert_eq!(
            contain.src_offsets,
            [
                vk::Offset3D { x: 0, y: 0, z: 0 },
                vk::Offset3D {
                    x: 3840,
                    y: 2160,
                    z: 1,
                }
            ]
        );
        assert!(contain.clear_before_blit);
        assert_eq!(contain.dst_offsets[0], vk::Offset3D { x: 55, y: 0, z: 0 });
        assert_eq!(
            contain.dst_offsets[1],
            vk::Offset3D {
                x: 944,
                y: 500,
                z: 1,
            }
        );

        let cover = scene_static_transfer_blit_region(FitMode::Cover, source, target).unwrap();
        assert!(!cover.clear_before_blit);
        assert_eq!(cover.dst_offsets[0], vk::Offset3D { x: 0, y: 0, z: 0 });
        assert_eq!(
            cover.dst_offsets[1],
            vk::Offset3D {
                x: 1000,
                y: 500,
                z: 1,
            }
        );
        assert_eq!(cover.src_offsets[0].x % 4, 0);
        assert_eq!(cover.src_offsets[0].y % 4, 0);
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
            Some(input),
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
        assert_close(floats[9], 2561.0);
        assert_close(floats[10], -53.166668);
        assert_close(floats[27], 2561.0);
        assert_close(floats[28], 1654.1666);
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
            NativeVulkanVulkanaliaSceneSampledImageVertex::new(
                [140.0, 20.0],
                [1.0 / 3.0, 0.0],
                0.5,
            ),
            NativeVulkanVulkanaliaSceneSampledImageVertex::new([40.0, 120.0], [0.0, 0.25], 0.5),
            NativeVulkanVulkanaliaSceneSampledImageVertex::new(
                [140.0, 120.0],
                [1.0 / 3.0, 0.25],
                0.5,
            ),
        ];
        let indices = vec![0, 1, 2, 2, 1, 3];
        let draw_steps = vec![NativeVulkanVulkanaliaSceneSampledImageDrawStep {
            layer_index: 7,
            resource_index: 0,
            first_index: 0,
            index_count: 6,
            blend_mode: SceneBlendMode::Alpha,
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
        assert_close(vertices[2].uv[0], 2.0 / 3.0);
        assert_close(vertices[2].uv[1], 0.5);
        assert_close(vertices[3].uv[0], 1.0);
        assert_close(vertices[3].uv[1], 0.5);
        assert_close(vertices[0].opacity, 0.5);
    }

    #[test]
    fn animated_atlas_updates_only_uv_bytes_without_retaining_base_vertices() {
        let vertices = vec![
            NativeVulkanVulkanaliaSceneSampledImageVertex::new([40.0, 20.0], [0.0, 0.0], 0.5),
            NativeVulkanVulkanaliaSceneSampledImageVertex::new(
                [140.0, 20.0],
                [1.0 / 3.0, 0.0],
                0.5,
            ),
            NativeVulkanVulkanaliaSceneSampledImageVertex::new([40.0, 120.0], [0.0, 0.25], 0.5),
            NativeVulkanVulkanaliaSceneSampledImageVertex::new(
                [140.0, 120.0],
                [1.0 / 3.0, 0.25],
                0.5,
            ),
        ];
        let indices = vec![0, 1, 2, 2, 1, 3];
        let draw_steps = vec![NativeVulkanVulkanaliaSceneSampledImageDrawStep {
            layer_index: 7,
            resource_index: 0,
            first_index: 0,
            index_count: 6,
            blend_mode: SceneBlendMode::Alpha,
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
        let animated_steps =
            scene_sampled_image_animated_uv_steps(&vertices, &indices, &draw_steps).unwrap();
        let vertex_bytes = scene_sampled_image_vertex_bytes(&vertices).unwrap();
        let mut bytes = vertex_bytes.to_vec();

        patch_scene_sampled_image_animated_uvs_in_bytes(
            &mut bytes,
            vertices.len(),
            &animated_steps,
            417,
        )
        .unwrap();

        assert_close(read_f32(&bytes, 0), 40.0);
        assert_close(read_f32(&bytes, 4), 20.0);
        assert_close(read_f32(&bytes, 8), 2.0 / 3.0);
        assert_close(read_f32(&bytes, 12), 0.25);
        assert_close(read_f32(&bytes, 16), 0.5);
        assert_close(read_f32(&bytes, 20), 1.0);
        assert_close(read_f32(&bytes, 24), 1.0);
        assert_close(read_f32(&bytes, 28), 1.0);
        assert_close(read_f32(&bytes, 32), 1.0);
        let fourth = 3 * SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES as usize;
        assert_close(read_f32(&bytes, fourth), 140.0);
        assert_close(read_f32(&bytes, fourth + 4), 120.0);
        assert_close(read_f32(&bytes, fourth + 8), 1.0);
        assert_close(read_f32(&bytes, fourth + 12), 0.5);
        assert_close(read_f32(&bytes, fourth + 16), 0.5);
        assert_close(read_f32(&bytes, fourth + 20), 1.0);
        assert_close(read_f32(&bytes, fourth + 24), 1.0);
        assert_close(read_f32(&bytes, fourth + 28), 1.0);
        assert_close(read_f32(&bytes, fourth + 32), 1.0);
    }

    #[test]
    fn sampled_image_vertex_buffer_count_uses_frame_slots_only_for_animated_atlas() {
        let static_steps = [NativeVulkanVulkanaliaSceneSampledImageDrawStep {
            layer_index: 0,
            resource_index: 0,
            first_index: 0,
            index_count: 6,
            blend_mode: SceneBlendMode::Alpha,
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

        assert_eq!(
            scene_sampled_image_vertex_buffer_count(&static_steps, 3, false),
            1
        );
        assert_eq!(
            scene_sampled_image_vertex_buffer_count(&static_steps, 3, true),
            3
        );
        assert_eq!(
            scene_sampled_image_vertex_buffer_count(&animated_steps, 3, false),
            3
        );
        assert_eq!(
            scene_sampled_image_vertex_buffer_count(&animated_steps, 0, false),
            1
        );
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.001,
            "expected {actual} to be within 0.001 of {expected}"
        );
    }

    fn read_f32(bytes: &[u8], offset: usize) -> f32 {
        f32::from_ne_bytes(
            bytes[offset..offset + 4]
                .try_into()
                .expect("test f32 byte range"),
        )
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

        let payload = scene_sampled_image_geometry_payload_from_input(input).unwrap();

        assert_eq!(
            payload.source_label,
            "scene-runtime-sampled-image-draw-plan"
        );
        assert_eq!(payload.vertex_count, 4);
        assert_eq!(payload.index_count, 6);
        assert_eq!(payload.quad_count, 1);
        assert_eq!(payload.vertex_bytes.len(), 144);
        assert_eq!(payload.index_bytes.len(), 24);
        let indices = payload
            .index_bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_ne_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();
        assert_eq!(indices, vec![0, 1, 2, 2, 1, 3]);
    }

    #[test]
    fn sampled_image_geometry_serializes_vertex_tint_after_opacity() {
        let input = NativeVulkanVulkanaliaSceneSampledImageGeometryInput::new(
            vec![
                NativeVulkanVulkanaliaSceneSampledImageVertex::new_tinted(
                    [0.0, 0.0],
                    [0.0, 0.0],
                    0.3,
                    [0.0, 0.0, 0.0, 1.0],
                ),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new_tinted(
                    [10.0, 0.0],
                    [1.0, 0.0],
                    0.3,
                    [0.0, 0.0, 0.0, 1.0],
                ),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new_tinted(
                    [0.0, 10.0],
                    [0.0, 1.0],
                    0.3,
                    [0.0, 0.0, 0.0, 1.0],
                ),
            ],
            vec![0, 1, 2],
            "sampled-shadow-tint",
        );

        let payload = scene_sampled_image_geometry_payload_from_input(input).unwrap();
        let floats = payload
            .vertex_bytes
            .chunks_exact(4)
            .map(|chunk| f32::from_ne_bytes(chunk.try_into().unwrap()))
            .collect::<Vec<_>>();

        assert_eq!(payload.vertex_bytes.len(), 108);
        assert_close(floats[4], 0.3);
        assert_eq!(&floats[5..9], &[0.0, 0.0, 0.0, 1.0]);
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
                    blend_mode: SceneBlendMode::Alpha,
                    fit: Some(FitMode::Cover),
                    texture_region: None,
                },
                NativeVulkanVulkanaliaSceneSampledImageDrawStep {
                    layer_index: 1,
                    resource_index: 1,
                    first_index: 6,
                    index_count: 6,
                    blend_mode: SceneBlendMode::Alpha,
                    fit: Some(FitMode::Tile),
                    texture_region: None,
                },
            ],
            "batched-scene-runtime-sampled-image-draw-plan",
        );

        let payload = scene_sampled_image_geometry_payload_from_input(input).unwrap();

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
    fn video_layer_geometry_accepts_distinct_n_source_scene_payload() {
        let input = NativeVulkanVulkanaliaSceneVideoLayerGeometryInput::new_batched(
            vec![
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([0.0, 0.0], [0.0, 0.0], 1.0),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([10.0, 0.0], [1.0, 0.0], 1.0),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([0.0, 10.0], [0.0, 1.0], 1.0),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([10.0, 10.0], [1.0, 1.0], 1.0),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([20.0, 20.0], [0.0, 0.0], 1.0),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([30.0, 20.0], [1.0, 0.0], 1.0),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([20.0, 30.0], [0.0, 1.0], 1.0),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([30.0, 30.0], [1.0, 1.0], 1.0),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([40.0, 40.0], [0.0, 0.0], 1.0),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([50.0, 40.0], [1.0, 0.0], 1.0),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([40.0, 50.0], [0.0, 1.0], 1.0),
                NativeVulkanVulkanaliaSceneSampledImageVertex::new([50.0, 50.0], [1.0, 1.0], 1.0),
            ],
            vec![0, 1, 2, 2, 1, 3, 4, 5, 6, 6, 5, 7, 8, 9, 10, 10, 9, 11],
            vec![
                PathBuf::from("/tmp/sky.mp4"),
                PathBuf::from("/tmp/character.mp4"),
                PathBuf::from("/tmp/effects.mp4"),
            ],
            vec![
                NativeVulkanVulkanaliaSceneVideoLayerDrawStep {
                    layer_index: 0,
                    resource_index: 0,
                    first_index: 0,
                    index_count: 6,
                    fit: Some(FitMode::Cover),
                },
                NativeVulkanVulkanaliaSceneVideoLayerDrawStep {
                    layer_index: 1,
                    resource_index: 1,
                    first_index: 6,
                    index_count: 6,
                    fit: Some(FitMode::Contain),
                },
                NativeVulkanVulkanaliaSceneVideoLayerDrawStep {
                    layer_index: 2,
                    resource_index: 2,
                    first_index: 12,
                    index_count: 6,
                    fit: Some(FitMode::Cover),
                },
            ],
            "scene-runtime-video-layer-draw-plan",
        );

        let payload = scene_video_layer_geometry_payload(
            input,
            vk::Extent2D {
                width: 1920,
                height: 1080,
            },
            None,
            FitMode::Cover,
        )
        .unwrap();

        assert_eq!(payload.vertex_count, 12);
        assert_eq!(payload.index_count, 18);
        assert_eq!(payload.quad_count, 3);
        assert_eq!(payload.source_count, 3);
        assert_eq!(payload.draw_steps.len(), 3);
        assert_eq!(payload.draw_steps[0].resource_index, 0);
        assert_eq!(payload.draw_steps[1].resource_index, 1);
        assert_eq!(payload.draw_steps[2].resource_index, 2);
        assert_eq!(payload.draw_steps[2].first_index, 12);
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

        let err = scene_sampled_image_geometry_payload_from_input(input).unwrap_err();

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

    fn static_transfer_test_options(
        fit: Option<FitMode>,
        geometry: Option<NativeVulkanVulkanaliaSceneSampledImageGeometryInput>,
        dynamic_geometry: Option<NativeVulkanVulkanaliaSceneSampledImageDynamicGeometry>,
    ) -> NativeVulkanVulkanaliaSceneSampledImagePresentOptions {
        NativeVulkanVulkanaliaSceneSampledImagePresentOptions {
            host: NativeWaylandHostOptions::default(),
            wait_configure_roundtrips: 0,
            duration: Duration::ZERO,
            target_max_fps: None,
            source: PathBuf::from("/tmp/static.gtex"),
            clear_color: NativeVulkanClearColor {
                r: 0.0,
                g: 0.0,
                b: 0.0,
                a: 1.0,
            },
            fit,
            solid_geometry: None,
            geometry,
            dynamic_solid_geometry: None,
            dynamic_geometry,
            scene_size: None,
            scene_fit: FitMode::Cover,
        }
    }
}
