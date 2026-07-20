use serde::Serialize;
use std::sync::Mutex;
use std::time::Instant;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{
    self, ExtDescriptorHeapExtensionDeviceCommands, HasBuilder, KhrSwapchainExtensionDeviceCommands,
};

#[path = "decoded_image_pipeline/frame_present.rs"]
mod frame_present;

pub(in crate::renderer::native_vulkan::vulkan) use frame_present::*;

use crate::renderer::native_vulkan::NativeVulkanClearColor;

use super::descriptor_heap::{
    NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_bind_info,
    native_vulkan_vulkanalia_descriptor_heap_sampler_bind_info,
    native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping_info,
};
pub(in crate::renderer::native_vulkan::vulkan) use super::present_timing::VulkanaliaPresentTimingConfig as VulkanaliaDecodedImagePresentTimingConfig;
pub(in crate::renderer::native_vulkan::vulkan) use super::render_present_descriptors::VulkanaliaDecodedImagePresentSamplerResources;
#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan::vulkan) use super::render_present_descriptors::native_vulkan_vulkanalia_create_ffmpeg_decoded_gpu_frame_present_sampler_resources;
#[cfg(feature = "native-vulkan-video")]
pub(in crate::renderer::native_vulkan::vulkan) use super::render_present_descriptors::native_vulkan_vulkanalia_destroy_decoded_image_present_sampler_resources;
use super::video_present_handoff::NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot;
use super::video_session_images::VulkanaliaVideoSessionResourceImage;

const FFMPEG_VULKAN_DECODE_REFERENCE: &str = "references/ffmpeg/libavcodec/vulkan_decode.c";

pub(in crate::renderer::native_vulkan::vulkan) const DECODED_IMAGE_PRESENT_TELEMETRY_RETAINED_FRAMES: usize = 0;
const DECODED_IMAGE_SCENE_VIDEO_LAYER_VERTEX_STRIDE_BYTES: u32 = 20;
const DECODED_IMAGE_SCENE_VIDEO_LAYER_PUSH_CONSTANT_BYTES: u32 = 8;

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
    pub pipeline_layout_created: bool,
    pub pipeline_created: bool,
    pub render_pass_compatibility: &'static str,
    pub primitive_topology: &'static str,
    pub vertex_shader_model: &'static str,
    pub fragment_shader_model: &'static str,
    pub descriptor_heap_only: bool,
    pub descriptor_model: &'static str,
    pub descriptor_heap_mapping_enabled: bool,
    pub descriptor_heap_plane_sampler_enabled: bool,
    pub descriptor_heap_pipeline_flag_enabled: bool,
    pub uses_pipeline_rendering_create_info: bool,
    pub uses_dynamic_rendering: bool,
    pub uses_plane_sampler_descriptors: bool,
    pub ffmpeg_reference: &'static str,
}

pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaDecodedImagePresentPipelineResources
{
    pub(in crate::renderer::native_vulkan::vulkan) pipeline_layout: vk::PipelineLayout,
    pub(in crate::renderer::native_vulkan::vulkan) pipeline: vk::Pipeline,
    scene_video_layer: VulkanaliaDecodedImageSceneVideoLayerPipelineResources,
    pub(in crate::renderer::native_vulkan::vulkan) snapshot:
        NativeVulkanVulkanaliaDecodedImagePresentPipelineSnapshot,
}

struct VulkanaliaDecodedImageSceneVideoLayerPipelineResources {
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
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

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_create_decoded_image_present_pipeline_resources(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
) -> Result<VulkanaliaDecodedImagePresentPipelineResources, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("decoded image present pipeline requires non-zero extent".to_owned());
    }
    if !descriptor_heap_plan.backend_ready {
        return Err(format!(
            "decoded image present pipeline requires a ready VK_EXT_descriptor_heap plan: {:?}",
            descriptor_heap_plan.blocking_reason
        ));
    }

    let pipeline_layout_info = vk::PipelineLayoutCreateInfo::builder();
    let pipeline_layout = unsafe { device.create_pipeline_layout(&pipeline_layout_info, None) }
        .map_err(|err| {
            format!("vkCreatePipelineLayout(vulkanalia decoded present dynamic rendering): {err:?}")
        })?;

    let result = (|| -> Result<VulkanaliaDecodedImagePresentPipelineResources, String> {
        let vertex_module = native_vulkan_vulkanalia_create_shader_module(
            device,
            &NATIVE_VULKAN_VULKANALIA_PLANE_PRESENT_VERTEX_SPIRV,
            "decoded present vertex",
        )?;
        let result = (|| -> Result<VulkanaliaDecodedImagePresentPipelineResources, String> {
            let fragment_module = native_vulkan_vulkanalia_create_shader_module(
                device,
                &NATIVE_VULKAN_VULKANALIA_PLANE_PRESENT_FRAGMENT_SPIRV,
                "decoded present fragment",
            )?;
            let result = (|| -> Result<VulkanaliaDecodedImagePresentPipelineResources, String> {
                let shader_entry = b"main\0";
                let descriptor_heap_mapping_enabled = true;
                let descriptor_heap_plane_sampler_enabled = true;
                let y_descriptor_heap_mapping =
                    native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_binding_mapping(
                        descriptor_heap_plan,
                        0,
                        0,
                    )?;
                let uv_descriptor_heap_mapping =
                    native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_binding_mapping(
                        descriptor_heap_plan,
                        1,
                        1,
                    )?;
                let descriptor_heap_mappings =
                    [y_descriptor_heap_mapping, uv_descriptor_heap_mapping];
                let mut descriptor_heap_mapping_info =
                    native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping_info(
                        &descriptor_heap_mappings,
                    )?;
                let fragment_stage_builder = vk::PipelineShaderStageCreateInfo::builder()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(fragment_module)
                    .name(shader_entry);
                let fragment_stage = if descriptor_heap_mapping_enabled {
                    let mut fragment_stage = fragment_stage_builder.build();
                    fragment_stage.next =
                        &mut descriptor_heap_mapping_info as *mut _ as *const std::ffi::c_void;
                    fragment_stage
                } else {
                    fragment_stage_builder.build()
                };
                let stages = [
                    vk::PipelineShaderStageCreateInfo::builder()
                        .stage(vk::ShaderStageFlags::VERTEX)
                        .module(vertex_module)
                        .name(shader_entry)
                        .build(),
                    fragment_stage,
                ];
                let vertex_input = vk::PipelineVertexInputStateCreateInfo::builder().build();
                let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
                    .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
                    .build();
                let viewport = vk::Viewport::builder()
                    .x(0.0)
                    .y(0.0)
                    .width(extent.width as f32)
                    .height(extent.height as f32)
                    .min_depth(0.0)
                    .max_depth(1.0)
                    .build();
                let scissor = vk::Rect2D::builder()
                    .offset(vk::Offset2D { x: 0, y: 0 })
                    .extent(extent)
                    .build();
                let viewports = [viewport];
                let scissors = [scissor];
                let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
                    .viewports(&viewports)
                    .scissors(&scissors)
                    .build();
                let rasterization = vk::PipelineRasterizationStateCreateInfo::builder()
                    .polygon_mode(vk::PolygonMode::FILL)
                    .cull_mode(vk::CullModeFlags::NONE)
                    .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
                    .line_width(1.0)
                    .build();
                let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
                    .rasterization_samples(vk::SampleCountFlags::_1)
                    .build();
                let color_attachment = vk::PipelineColorBlendAttachmentState::builder()
                    .color_write_mask(
                        vk::ColorComponentFlags::R
                            | vk::ColorComponentFlags::G
                            | vk::ColorComponentFlags::B
                            | vk::ColorComponentFlags::A,
                    )
                    .blend_enable(false)
                    .build();
                let color_attachments = [color_attachment];
                let color_blend = vk::PipelineColorBlendStateCreateInfo::builder()
                    .attachments(&color_attachments)
                    .build();
                let color_attachment_formats = [target_format];
                let mut rendering_info = vk::PipelineRenderingCreateInfo::builder()
                    .color_attachment_formats(&color_attachment_formats)
                    .build();
                let mut pipeline_flags2 = vk::PipelineCreateFlags2CreateInfo::builder()
                    .flags(vk::PipelineCreateFlags2::DESCRIPTOR_HEAP_EXT)
                    .build();
                let mut pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
                    .stages(&stages)
                    .vertex_input_state(&vertex_input)
                    .input_assembly_state(&input_assembly)
                    .viewport_state(&viewport_state)
                    .rasterization_state(&rasterization)
                    .multisample_state(&multisample)
                    .color_blend_state(&color_blend)
                    // VK_PIPELINE_CREATE_2_DESCRIPTOR_HEAP_BIT_EXT requires layout to be
                    // VK_NULL_HANDLE (VUID-VkGraphicsPipelineCreateInfo-flags-11311); the
                    // descriptor bindings come from the pushed mapping info, not a layout.
                    .layout(vk::PipelineLayout::null())
                    .render_pass(vk::RenderPass::null())
                    .subpass(0)
                    .push_next(&mut rendering_info);
                if descriptor_heap_mapping_enabled {
                    pipeline_info = pipeline_info.push_next(&mut pipeline_flags2);
                }
                let pipeline_info = pipeline_info.build();
                let (pipelines, _success_code) = unsafe {
                    device.create_graphics_pipelines(
                        vk::PipelineCache::null(),
                        &[pipeline_info],
                        None,
                    )
                }
                .map_err(|err| {
                    format!(
                        "vkCreateGraphicsPipelines(vulkanalia decoded present dynamic rendering): {err:?}"
                    )
                })?;
                let pipeline = pipelines[0];
                let scene_video_layer =
                    match native_vulkan_vulkanalia_create_decoded_image_scene_video_layer_pipeline_resources(
                        device,
                        target_format,
                        extent,
                        descriptor_heap_plan,
                    ) {
                        Ok(resources) => resources,
                        Err(err) => {
                            unsafe {
                                device.destroy_pipeline(pipeline, None);
                            }
                            return Err(err);
                        }
                    };
                Ok(VulkanaliaDecodedImagePresentPipelineResources {
                    pipeline_layout,
                    pipeline,
                    scene_video_layer,
                    snapshot: NativeVulkanVulkanaliaDecodedImagePresentPipelineSnapshot {
                        binding: "vulkanalia",
                        route: "decoded-image-dynamic-rendering-present-pipeline",
                        target_format: format!("{target_format:?}"),
                        extent: (extent.width, extent.height),
                        shader_modules_created: true,
                        pipeline_layout_created: true,
                        pipeline_created: true,
                        render_pass_compatibility: "dynamic-rendering-no-render-pass",
                        primitive_topology: "fullscreen-triangle",
                        vertex_shader_model: "gl_VertexIndex fullscreen triangle",
                        fragment_shader_model: "two retained plane sampler2DArray descriptors with instance-index layer selection",
                        descriptor_heap_only: true,
                        descriptor_model: "VK_EXT_descriptor_heap",
                        descriptor_heap_mapping_enabled,
                        descriptor_heap_plane_sampler_enabled,
                        descriptor_heap_pipeline_flag_enabled: true,
                        uses_pipeline_rendering_create_info: true,
                        uses_dynamic_rendering: true,
                        uses_plane_sampler_descriptors: true,
                        ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
                    },
                })
            })();
            unsafe {
                device.destroy_shader_module(fragment_module, None);
            }
            result
        })();
        unsafe {
            device.destroy_shader_module(vertex_module, None);
        }
        result
    })();

    if result.is_err() {
        unsafe {
            device.destroy_pipeline_layout(pipeline_layout, None);
        }
    }
    result
}

fn native_vulkan_vulkanalia_create_decoded_image_scene_video_layer_pipeline_resources(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
) -> Result<VulkanaliaDecodedImageSceneVideoLayerPipelineResources, String> {
    let push_range = vk::PushConstantRange::builder()
        .stage_flags(vk::ShaderStageFlags::VERTEX)
        .offset(0)
        .size(DECODED_IMAGE_SCENE_VIDEO_LAYER_PUSH_CONSTANT_BYTES)
        .build();
    let push_ranges = [push_range];
    let pipeline_layout_info =
        vk::PipelineLayoutCreateInfo::builder().push_constant_ranges(&push_ranges);
    let pipeline_layout = unsafe { device.create_pipeline_layout(&pipeline_layout_info, None) }
        .map_err(|err| {
            format!("vkCreatePipelineLayout(vulkanalia decoded scene video layer): {err:?}")
        })?;

    let result = (|| -> Result<VulkanaliaDecodedImageSceneVideoLayerPipelineResources, String> {
        let vertex_module = native_vulkan_vulkanalia_create_shader_module(
            device,
            &NATIVE_VULKAN_VULKANALIA_PLANE_SCENE_VIDEO_LAYER_VERTEX_SPIRV,
            "decoded scene video layer vertex",
        )?;
        let result =
            (|| -> Result<VulkanaliaDecodedImageSceneVideoLayerPipelineResources, String> {
                let fragment_module = native_vulkan_vulkanalia_create_shader_module(
                    device,
                    &NATIVE_VULKAN_VULKANALIA_PLANE_SCENE_VIDEO_LAYER_FRAGMENT_SPIRV,
                    "decoded scene video layer fragment",
                )?;
                let result =
                (|| -> Result<VulkanaliaDecodedImageSceneVideoLayerPipelineResources, String> {
                    let shader_entry = b"main\0";
                    let y_descriptor_heap_mapping =
                        native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_binding_mapping(
                            descriptor_heap_plan,
                            0,
                            0,
                        )?;
                    let uv_descriptor_heap_mapping =
                        native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_binding_mapping(
                            descriptor_heap_plan,
                            1,
                            1,
                        )?;
                    let descriptor_heap_mappings =
                        [y_descriptor_heap_mapping, uv_descriptor_heap_mapping];
                    let mut descriptor_heap_mapping_info =
                        native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping_info(
                            &descriptor_heap_mappings,
                        )?;
                    let mut fragment_stage = vk::PipelineShaderStageCreateInfo::builder()
                        .stage(vk::ShaderStageFlags::FRAGMENT)
                        .module(fragment_module)
                        .name(shader_entry)
                        .build();
                    fragment_stage.next = &mut descriptor_heap_mapping_info as *mut _ as *const std::ffi::c_void;
                    let stages = [
                        vk::PipelineShaderStageCreateInfo::builder()
                            .stage(vk::ShaderStageFlags::VERTEX)
                            .module(vertex_module)
                            .name(shader_entry)
                            .build(),
                        fragment_stage,
                    ];
                    let binding = vk::VertexInputBindingDescription::builder()
                        .binding(0)
                        .stride(DECODED_IMAGE_SCENE_VIDEO_LAYER_VERTEX_STRIDE_BYTES)
                        .input_rate(vk::VertexInputRate::VERTEX)
                        .build();
                    let attributes = [
                        vk::VertexInputAttributeDescription::builder()
                            .location(0)
                            .binding(0)
                            .format(vk::Format::R32G32_SFLOAT)
                            .offset(0)
                            .build(),
                        vk::VertexInputAttributeDescription::builder()
                            .location(1)
                            .binding(0)
                            .format(vk::Format::R32G32_SFLOAT)
                            .offset(8)
                            .build(),
                        vk::VertexInputAttributeDescription::builder()
                            .location(2)
                            .binding(0)
                            .format(vk::Format::R32_SFLOAT)
                            .offset(16)
                            .build(),
                    ];
                    let bindings = [binding];
                    let vertex_input = vk::PipelineVertexInputStateCreateInfo::builder()
                        .vertex_binding_descriptions(&bindings)
                        .vertex_attribute_descriptions(&attributes)
                        .build();
                    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
                        .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
                        .build();
                    let viewport = vk::Viewport::builder()
                        .x(0.0)
                        .y(0.0)
                        .width(extent.width as f32)
                        .height(extent.height as f32)
                        .min_depth(0.0)
                        .max_depth(1.0)
                        .build();
                    let scissor = vk::Rect2D::builder()
                        .offset(vk::Offset2D { x: 0, y: 0 })
                        .extent(extent)
                        .build();
                    let viewports = [viewport];
                    let scissors = [scissor];
                    let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
                        .viewports(&viewports)
                        .scissors(&scissors)
                        .build();
                    let rasterization = vk::PipelineRasterizationStateCreateInfo::builder()
                        .polygon_mode(vk::PolygonMode::FILL)
                        .cull_mode(vk::CullModeFlags::NONE)
                        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
                        .line_width(1.0)
                        .build();
                    let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
                        .rasterization_samples(vk::SampleCountFlags::_1)
                        .build();
                    let color_attachment = vk::PipelineColorBlendAttachmentState::builder()
                        .color_write_mask(
                            vk::ColorComponentFlags::R
                                | vk::ColorComponentFlags::G
                                | vk::ColorComponentFlags::B
                                | vk::ColorComponentFlags::A,
                        )
                        .blend_enable(true)
                        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
                        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                        .color_blend_op(vk::BlendOp::ADD)
                        .src_alpha_blend_factor(vk::BlendFactor::ONE)
                        .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
                        .alpha_blend_op(vk::BlendOp::ADD)
                        .build();
                    let color_attachments = [color_attachment];
                    let color_blend = vk::PipelineColorBlendStateCreateInfo::builder()
                        .attachments(&color_attachments)
                        .build();
                    let color_attachment_formats = [target_format];
                    let mut rendering_info = vk::PipelineRenderingCreateInfo::builder()
                        .color_attachment_formats(&color_attachment_formats)
                        .build();
                    let mut pipeline_flags2 = vk::PipelineCreateFlags2CreateInfo::builder()
                        .flags(vk::PipelineCreateFlags2::DESCRIPTOR_HEAP_EXT)
                        .build();
                    let mut pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
                        .stages(&stages)
                        .vertex_input_state(&vertex_input)
                        .input_assembly_state(&input_assembly)
                        .viewport_state(&viewport_state)
                        .rasterization_state(&rasterization)
                        .multisample_state(&multisample)
                        .color_blend_state(&color_blend)
                        .layout(pipeline_layout)
                        .render_pass(vk::RenderPass::null())
                        .subpass(0)
                        .push_next(&mut rendering_info);
                    pipeline_info = pipeline_info.push_next(&mut pipeline_flags2);
                    let pipeline_info = pipeline_info.build();
                    let (pipelines, _success_code) = unsafe {
                        device.create_graphics_pipelines(
                            vk::PipelineCache::null(),
                            &[pipeline_info],
                            None,
                        )
                    }
                    .map_err(|err| {
                        format!(
                            "vkCreateGraphicsPipelines(vulkanalia decoded scene video layer): {err:?}"
                        )
                    })?;
                    Ok(VulkanaliaDecodedImageSceneVideoLayerPipelineResources {
                        pipeline_layout,
                        pipeline: pipelines[0],
                    })
                })();
                unsafe {
                    device.destroy_shader_module(fragment_module, None);
                }
                result
            })();
        unsafe {
            device.destroy_shader_module(vertex_module, None);
        }
        result
    })();

    if result.is_err() {
        unsafe {
            device.destroy_pipeline_layout(pipeline_layout, None);
        }
    }
    result
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_destroy_decoded_image_present_pipeline_resources(
    device: &Device,
    resources: VulkanaliaDecodedImagePresentPipelineResources,
) {
    unsafe {
        device.destroy_pipeline(resources.scene_video_layer.pipeline, None);
        device.destroy_pipeline_layout(resources.scene_video_layer.pipeline_layout, None);
        device.destroy_pipeline(resources.pipeline, None);
        device.destroy_pipeline_layout(resources.pipeline_layout, None);
    }
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
