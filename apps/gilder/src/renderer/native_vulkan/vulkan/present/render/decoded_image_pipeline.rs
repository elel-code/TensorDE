use serde::Serialize;
use std::sync::Mutex;
use std::time::Instant;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{
    self, ExtDescriptorHeapExtensionDeviceCommands, HasBuilder, KhrSwapchainExtensionDeviceCommands,
};

#[path = "decoded_image_pipeline/frame_present.rs"]
mod frame_present;
#[path = "decoded_image_pipeline/pipeline_creation.rs"]
mod pipeline_creation;

pub(in crate::renderer::native_vulkan::vulkan) use frame_present::*;
#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan::vulkan) use pipeline_creation::*;

use crate::renderer::native_vulkan::NativeVulkanClearColor;

use super::descriptor_heap::{
    NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    native_vulkan_vulkanalia_descriptor_heap_resource_bind_info,
    native_vulkan_vulkanalia_descriptor_heap_sampler_bind_info,
};
pub(in crate::renderer::native_vulkan::vulkan) use super::present_timing::VulkanaliaPresentTimingConfig as VulkanaliaDecodedImagePresentTimingConfig;
pub(in crate::renderer::native_vulkan::vulkan) use super::render_present_descriptors::VulkanaliaDecodedImagePresentSamplerResources;
#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan::vulkan) use super::render_present_descriptors::native_vulkan_vulkanalia_create_ffmpeg_decoded_gpu_frame_present_sampler_resources;
#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan::vulkan) use super::render_present_descriptors::native_vulkan_vulkanalia_destroy_decoded_image_present_sampler_resources;
use super::video_present_handoff::NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot;
use super::video_session_images::VulkanaliaVideoSessionResourceImage;

const FFMPEG_VULKAN_DECODE_REFERENCE: &str = "references/gilder/ffmpeg/libavcodec/vulkan_decode.c";

pub(in crate::renderer::native_vulkan::vulkan) const DECODED_IMAGE_PRESENT_TELEMETRY_RETAINED_FRAMES: usize = 0;
const DECODED_IMAGE_SCENE_VIDEO_LAYER_VERTEX_STRIDE_BYTES: u32 = 20;

#[derive(Clone, Copy)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneVideoLayerDrawCommand {
    pub(in crate::renderer::native_vulkan::vulkan) first_index: u32,
    pub(in crate::renderer::native_vulkan::vulkan) index_count: u32,
    pub(in crate::renderer::native_vulkan::vulkan) resource_index: u32,
}

#[derive(Clone, Copy)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneVideoLayerFrameDraw<'a> {
    pub(in crate::renderer::native_vulkan::vulkan) vertex_buffer: vk::Buffer,
    pub(in crate::renderer::native_vulkan::vulkan) index_buffer: vk::Buffer,
    pub(in crate::renderer::native_vulkan::vulkan) draw_commands:
        &'a [VulkanaliaSceneVideoLayerDrawCommand],
}

#[derive(Clone, Copy)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneVideoOverlayFrameDraw<'a> {
    pub(in crate::renderer::native_vulkan::vulkan) video_layer:
        Option<VulkanaliaSceneVideoLayerFrameDraw<'a>>,
}

fn native_vulkan_vulkanalia_elapsed_micros(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_micros()).unwrap_or(u64::MAX)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaDecodedImagePresentPipelineSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub target_format: String,
    pub extent: (u32, u32),
    pub shader_modules_created: bool,
    pub pipeline_layout_null: bool,
    pub pipeline_created: bool,
    pub render_pass_compatibility: &'static str,
    pub primitive_topology: &'static str,
    pub vertex_shader_model: &'static str,
    pub fragment_shader_model: &'static str,
    pub descriptor_heap_only: bool,
    pub descriptor_model: &'static str,
    pub native_descriptor_push_enabled: bool,
    pub descriptor_heap_plane_sampler_enabled: bool,
    pub descriptor_heap_pipeline_flag_enabled: bool,
    pub uses_pipeline_rendering_create_info: bool,
    pub uses_dynamic_rendering: bool,
    pub uses_plane_sampler_descriptors: bool,
    pub ffmpeg_reference: &'static str,
}

pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaDecodedImagePresentPipelineResources
{
    pub(in crate::renderer::native_vulkan::vulkan) pipeline: vk::Pipeline,
    descriptor_push: Vec<u8>,
    scene_video_layer: VulkanaliaDecodedImageSceneVideoLayerPipelineResources,
    pub(in crate::renderer::native_vulkan::vulkan) snapshot:
        NativeVulkanVulkanaliaDecodedImagePresentPipelineSnapshot,
}

struct VulkanaliaDecodedImageSceneVideoLayerPipelineResources {
    pipeline: vk::Pipeline,
    descriptor_push: Vec<u8>,
}

pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaDecodedImagePresentFrameResources {
    swapchain_image_views: Vec<vk::ImageView>,
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    image_available: Vec<vk::Semaphore>,
    render_finished: Vec<vk::Semaphore>,
    in_flight: Vec<vk::Fence>,
    swapchain_image_in_flight: Mutex<Vec<vk::Fence>>,
    // Timeline semaphore signalled by the video-queue decode submit and waited on by
    // the present submit, providing the decode->present cross-queue dependency.
    decode_complete: vk::Semaphore,
    present_queue_family_index: u32,
}

impl VulkanaliaDecodedImagePresentFrameResources {
    pub(in crate::renderer::native_vulkan::vulkan) fn decode_complete_semaphore(
        &self,
    ) -> vk::Semaphore {
        self.decode_complete
    }
}

#[derive(Clone, Copy)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaDecodedImagePresentImageSource {
    pub(in crate::renderer::native_vulkan::vulkan) image: vk::Image,
    pub(in crate::renderer::native_vulkan::vulkan) array_layers: u32,
    pub(in crate::renderer::native_vulkan::vulkan) current_layout: vk::ImageLayout,
    pub(in crate::renderer::native_vulkan::vulkan) restore_layout: vk::ImageLayout,
    pub(in crate::renderer::native_vulkan::vulkan) queue_family_index: u32,
}

impl VulkanaliaDecodedImagePresentImageSource {
    fn from_resource_image(resource_image: &VulkanaliaVideoSessionResourceImage) -> Self {
        Self {
            image: resource_image.image,
            array_layers: resource_image.snapshot.array_layers,
            current_layout: vk::ImageLayout::VIDEO_DECODE_DPB_KHR,
            restore_layout: vk::ImageLayout::VIDEO_DECODE_DPB_KHR,
            queue_family_index: vk::QUEUE_FAMILY_IGNORED,
        }
    }
}

pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaDecodedImagePresentSource<'a> {
    pub(in crate::renderer::native_vulkan::vulkan) image: VulkanaliaDecodedImagePresentImageSource,
    pub(in crate::renderer::native_vulkan::vulkan) sampler:
        &'a VulkanaliaDecodedImagePresentSamplerResources,
    pub(in crate::renderer::native_vulkan::vulkan) sampled_array_layer: u32,
}

#[derive(Clone, Copy)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaDecodedImagePresentDecodeWait {
    pub(in crate::renderer::native_vulkan::vulkan) semaphore: vk::Semaphore,
    pub(in crate::renderer::native_vulkan::vulkan) value: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub present_frame_index: u32,
    pub sampled_array_layer: u32,
    pub sampled_array_layer_source: &'static str,
    pub source_frame_pts_ns: Option<u64>,
    pub source_frame_duration_ns: Option<u64>,
    pub source_frame_pts_ms: Option<u64>,
    pub source_frame_duration_ms: Option<u64>,
    pub display_order_key: i64,
    pub display_order_key_source: &'static str,
    pub pacing_sleep_micros: u64,
    pub pacing_clock_model: &'static str,
    pub present_call_total_micros: u64,
    pub present_wait_frame_slot_micros: u64,
    pub present_acquire_next_image_micros: u64,
    pub present_record_command_buffer_micros: u64,
    pub present_submit_command_buffer_micros: u64,
    pub present_queue_present_micros: u64,
    pub present_wait_after_queue_present_micros: u64,
    pub present_frame_slot: u32,
    pub present_sync_model: &'static str,
    pub wait_idle_after_present: bool,
    pub present_id: Option<u64>,
    pub present_id_mode: &'static str,
    pub uses_present_id2: bool,
    pub present_wait2_available: bool,
    pub present_wait_after_present: bool,
    pub swapchain_image_index: u32,
    pub swapchain_image_view_count: usize,
    pub target_format: String,
    pub extent: (u32, u32),
    pub clear_color: [f32; 4],
    pub command_buffer_recorded: bool,
    pub submitted: bool,
    pub presented: bool,
    pub decoded_image_layout_transition: &'static str,
    pub swapchain_layout_transition: &'static str,
    pub render_model: &'static str,
    pub command_order: Vec<&'static str>,
    pub uses_pipeline_rendering_create_info: bool,
    pub uses_dynamic_rendering: bool,
    pub uses_synchronization2: bool,
    pub uses_submit2: bool,
    pub zero_copy_presented: bool,
    pub descriptor_model: &'static str,
    pub ffmpeg_reference: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanVulkanaliaDecodedImagePresentSlowFrameSnapshot {
    pub present_frame_index: u32,
    pub present_frame_slot: u32,
    pub sampled_array_layer: u32,
    pub delta_micros: u64,
    pub present_call_total_micros: u64,
    pub present_record_command_buffer_micros: u64,
    pub present_submit_command_buffer_micros: u64,
    pub present_queue_present_micros: u64,
    pub present_wait_frame_slot_micros: u64,
    pub source_frame_pts_ns: Option<u64>,
    pub display_order_key: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub execution_model: &'static str,
    pub ffmpeg_thread_model: &'static str,
    pub ffmpeg_read_thread_active: bool,
    pub video_decode_worker_active: bool,
    pub present_worker_active: bool,
    pub decode_thread_count: u32,
    pub decode_async_exec_depth: u32,
    pub requested_present_frame_count: u32,
    pub submitted_present_frame_count: u32,
    pub presented_frame_count: u32,
    pub average_present_fps: f64,
    pub average_present_teardown_inclusive_fps: f64,
    pub present_interval_elapsed_micros: u64,
    pub present_teardown_inclusive_elapsed_micros: u64,
    pub present_delta_min_micros: Option<u64>,
    pub present_delta_max_micros: Option<u64>,
    pub present_delta_over_6250us_count: u32,
    pub present_delta_over_8334us_count: u32,
    pub slow_frame_telemetry_limit: usize,
    pub slow_frames: Vec<NativeVulkanVulkanaliaDecodedImagePresentSlowFrameSnapshot>,
    pub retained_frame_telemetry_limit: usize,
    pub distinct_sampled_array_layer_count: u32,
    pub sampled_array_layers_head: Vec<u32>,
    pub sampled_array_layers_tail: Vec<u32>,
    pub source_frame_pts_ns_head: Vec<Option<u64>>,
    pub source_frame_pts_ns_tail: Vec<Option<u64>>,
    pub source_frame_pts_delta_min_ns: Option<u64>,
    pub source_frame_pts_delta_max_ns: Option<u64>,
    pub source_frame_duration_ns_head: Vec<Option<u64>>,
    pub source_frame_duration_ns_tail: Vec<Option<u64>>,
    pub source_frame_pts_ms_head: Vec<Option<u64>>,
    pub source_frame_pts_ms_tail: Vec<Option<u64>>,
    pub source_frame_pts_delta_min_ms: Option<u64>,
    pub source_frame_pts_delta_max_ms: Option<u64>,
    pub source_frame_duration_ms_head: Vec<Option<u64>>,
    pub source_frame_duration_ms_tail: Vec<Option<u64>>,
    pub display_order_keys_head: Vec<i64>,
    pub display_order_keys_tail: Vec<i64>,
    pub display_order_key_sources_head: Vec<&'static str>,
    pub display_order_key_sources_tail: Vec<&'static str>,
    pub present_ids_head: Vec<Option<u64>>,
    pub present_ids_tail: Vec<Option<u64>>,
    pub frame_sleep_count: u32,
    pub missed_frame_pacing_count: u32,
    pub total_pacing_sleep_micros: u64,
    pub present_sleep_guard_micros: u64,
    pub present_spin_guard_micros: u64,
    pub total_present_call_micros: u64,
    pub max_present_call_micros: u64,
    pub total_present_wait_frame_slot_micros: u64,
    pub max_present_wait_frame_slot_micros: u64,
    pub total_present_acquire_next_image_micros: u64,
    pub max_present_acquire_next_image_micros: u64,
    pub total_present_record_command_buffer_micros: u64,
    pub max_present_record_command_buffer_micros: u64,
    pub total_present_submit_command_buffer_micros: u64,
    pub max_present_submit_command_buffer_micros: u64,
    pub total_present_queue_present_micros: u64,
    pub max_present_queue_present_micros: u64,
    pub total_present_wait_after_queue_present_micros: u64,
    pub max_present_wait_after_queue_present_micros: u64,
    pub pts_monotonic: bool,
    pub display_order_monotonic: bool,
    pub uses_present_id2: bool,
    pub present_wait2_available: bool,
    pub present_wait_after_present: bool,
    pub present_handoff: NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot,
    pub latest_draw: Option<NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot>,
    pub draws_head: Vec<NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot>,
    pub draws_tail: Vec<NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot>,
    pub frame_order_model: &'static str,
    pub present_resource_reuse_model: &'static str,
    pub ffmpeg_retained_avframe_count: u32,
    pub ffmpeg_retained_avframe_peak_count: u32,
    pub descriptor_sampler_cache_entry_count: u32,
    pub descriptor_sampler_cache_peak_entry_count: u32,
    pub descriptor_sampler_cache_rewrite_count: u32,
    pub descriptor_sampler_cache_recreate_count: u32,
    pub descriptor_sampler_cache_resource_heap_bytes: u64,
    pub descriptor_sampler_cache_sampler_heap_bytes: u64,
    pub descriptor_sampler_cache_total_heap_bytes: u64,
    pub telemetry_retention_model: &'static str,
    pub all_zero_copy_presented: bool,
    pub uses_dynamic_rendering: bool,
    pub uses_synchronization2: bool,
    pub uses_submit2: bool,
    pub ffmpeg_reference: &'static str,
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_create_decoded_image_present_frame_resources(
    device: &Device,
    swapchain_images: &[vk::Image],
    swapchain_format: vk::Format,
    queue_family_index: u32,
) -> Result<VulkanaliaDecodedImagePresentFrameResources, String> {
    if swapchain_images.is_empty() {
        return Err("decoded image present requires at least one swapchain image".to_owned());
    }

    let mut swapchain_image_views = Vec::new();
    let mut command_pool = vk::CommandPool::null();
    let mut image_available = Vec::new();
    let mut render_finished = Vec::new();
    let mut in_flight = Vec::new();
    let mut decode_complete = vk::Semaphore::null();

    let result = (|| -> Result<VulkanaliaDecodedImagePresentFrameResources, String> {
        swapchain_image_views = native_vulkan_vulkanalia_create_present_swapchain_image_views(
            device,
            swapchain_images,
            swapchain_format,
        )?;

        let command_pool_info = vk::CommandPoolCreateInfo::builder()
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
            .queue_family_index(queue_family_index);
        command_pool =
            unsafe { device.create_command_pool(&command_pool_info, None) }.map_err(|err| {
                format!("vkCreateCommandPool(vulkanalia decoded image present): {err:?}")
            })?;
        let command_buffer_info = vk::CommandBufferAllocateInfo::builder()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(swapchain_images.len() as u32);
        let command_buffers = unsafe { device.allocate_command_buffers(&command_buffer_info) }
            .map_err(|err| {
                format!("vkAllocateCommandBuffers(vulkanalia decoded image present): {err:?}")
            })?;

        let semaphore_info = vk::SemaphoreCreateInfo::builder();
        let fence_info = vk::FenceCreateInfo::builder().flags(vk::FenceCreateFlags::SIGNALED);
        for frame_slot in 0..swapchain_images.len() {
            image_available.push(
                unsafe { device.create_semaphore(&semaphore_info, None) }.map_err(|err| {
                    format!(
                        "vkCreateSemaphore(image_available slot {frame_slot} vulkanalia decoded image present): {err:?}"
                    )
                })?,
            );
            render_finished.push(
                unsafe { device.create_semaphore(&semaphore_info, None) }.map_err(|err| {
                    format!(
                        "vkCreateSemaphore(render_finished slot {frame_slot} vulkanalia decoded image present): {err:?}"
                    )
                })?,
            );
            in_flight.push(
                unsafe { device.create_fence(&fence_info, None) }.map_err(|err| {
                    format!(
                        "vkCreateFence(slot {frame_slot} vulkanalia decoded image present): {err:?}"
                    )
                })?,
            );
        }

        let mut decode_complete_type_info = vk::SemaphoreTypeCreateInfo::builder()
            .semaphore_type(vk::SemaphoreType::TIMELINE)
            .initial_value(0);
        let decode_complete_info =
            vk::SemaphoreCreateInfo::builder().push_next(&mut decode_complete_type_info);
        decode_complete = unsafe { device.create_semaphore(&decode_complete_info, None) }
            .map_err(|err| {
                format!("vkCreateSemaphore(decode_complete timeline vulkanalia decoded image present): {err:?}")
            })?;

        Ok(VulkanaliaDecodedImagePresentFrameResources {
            swapchain_image_views: std::mem::take(&mut swapchain_image_views),
            command_pool,
            command_buffers,
            image_available: std::mem::take(&mut image_available),
            render_finished: std::mem::take(&mut render_finished),
            in_flight: std::mem::take(&mut in_flight),
            swapchain_image_in_flight: Mutex::new(vec![vk::Fence::null(); swapchain_images.len()]),
            decode_complete,
            present_queue_family_index: queue_family_index,
        })
    })();

    if result.is_err() {
        native_vulkan_vulkanalia_destroy_partial_decoded_image_present_frame_resources(
            device,
            swapchain_image_views,
            command_pool,
            image_available,
            render_finished,
            in_flight,
            decode_complete,
        );
    }

    result
}
