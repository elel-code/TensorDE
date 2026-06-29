#![allow(dead_code)]

use serde::Serialize;
use std::sync::Mutex;
use std::time::Instant;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{
    self, ExtDescriptorHeapExtensionDeviceCommands, HasBuilder, KhrSwapchainExtensionDeviceCommands,
};

use crate::renderer::native_vulkan::NativeVulkanClearColor;

use super::super::scene::present::{
    VulkanaliaSceneVideoLayerFrameDraw, VulkanaliaSceneVideoOverlayFrameDraw,
    native_vulkan_vulkanalia_record_scene_video_overlay_draws_inside_rendering,
};
use super::descriptor_heap::{
    NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    VulkanaliaDescriptorHeapImageSamplerResources,
    native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_bind_info,
    native_vulkan_vulkanalia_descriptor_heap_sampler_bind_info,
};
pub(in crate::renderer::native_vulkan::vulkan) use super::present_timing::VulkanaliaPresentTimingConfig as VulkanaliaDecodedImagePresentTimingConfig;
pub(in crate::renderer::native_vulkan::vulkan) use super::render_present_descriptors::{
    NativeVulkanVulkanaliaDecodedImagePresentSamplerSnapshot,
    VulkanaliaDecodedImagePresentSamplerResources,
    native_vulkan_vulkanalia_create_decoded_image_present_sampler_resources,
    native_vulkan_vulkanalia_destroy_decoded_image_present_sampler_resources,
    native_vulkan_vulkanalia_retarget_decoded_image_present_sampler_layer,
};
use super::video_decode_submit::FFMPEG_VULKAN_DECODE_REFERENCE;
use super::video_present_handoff::NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot;
use super::video_session_images::VulkanaliaVideoSessionResourceImage;

pub(in crate::renderer::native_vulkan::vulkan) const DECODED_IMAGE_PRESENT_TELEMETRY_RETAINED_FRAMES: usize = 0;
const DECODED_IMAGE_SCENE_VIDEO_LAYER_VERTEX_STRIDE_BYTES: u32 = 20;
const DECODED_IMAGE_SCENE_VIDEO_LAYER_PUSH_CONSTANT_BYTES: u32 = 8;

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
    pub descriptor_sets: u32,
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
}

impl VulkanaliaDecodedImagePresentFrameResources {
    pub(in crate::renderer::native_vulkan::vulkan) fn decode_complete_semaphore(
        &self,
    ) -> vk::Semaphore {
        self.decode_complete
    }
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
                    vk::ShaderDescriptorSetAndBindingMappingInfoEXT::builder()
                        .mappings(&descriptor_heap_mappings)
                        .build();
                let mut fragment_stage = vk::PipelineShaderStageCreateInfo::builder()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(fragment_module)
                    .name(shader_entry);
                if descriptor_heap_mapping_enabled {
                    fragment_stage = fragment_stage.push_next(&mut descriptor_heap_mapping_info);
                }
                let stages = [
                    vk::PipelineShaderStageCreateInfo::builder()
                        .stage(vk::ShaderStageFlags::VERTEX)
                        .module(vertex_module)
                        .name(shader_entry)
                        .build(),
                    fragment_stage.build(),
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
                        descriptor_sets: 0,
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
                        vk::ShaderDescriptorSetAndBindingMappingInfoEXT::builder()
                            .mappings(&descriptor_heap_mappings)
                            .build();
                    let fragment_stage = vk::PipelineShaderStageCreateInfo::builder()
                        .stage(vk::ShaderStageFlags::FRAGMENT)
                        .module(fragment_module)
                        .name(shader_entry)
                        .push_next(&mut descriptor_heap_mapping_info);
                    let stages = [
                        vk::PipelineShaderStageCreateInfo::builder()
                            .stage(vk::ShaderStageFlags::VERTEX)
                            .module(vertex_module)
                            .name(shader_entry)
                            .build(),
                        fragment_stage.build(),
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

#[allow(clippy::too_many_arguments)]
pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_present_decoded_image_once(
    device: &Device,
    queue: vk::Queue,
    queue_family_index: u32,
    swapchain: vk::SwapchainKHR,
    swapchain_images: &[vk::Image],
    swapchain_format: vk::Format,
    swapchain_extent: vk::Extent2D,
    resource_image: &VulkanaliaVideoSessionResourceImage,
    sampler: &VulkanaliaDecodedImagePresentSamplerResources,
    pipeline: &VulkanaliaDecodedImagePresentPipelineResources,
    present_timing: VulkanaliaDecodedImagePresentTimingConfig,
    clear_color: NativeVulkanClearColor,
) -> Result<NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot, String> {
    let frame_resources = native_vulkan_vulkanalia_create_decoded_image_present_frame_resources(
        device,
        swapchain_images,
        swapchain_format,
        queue_family_index,
    )?;
    let result = native_vulkan_vulkanalia_present_decoded_image_frame(
        device,
        queue,
        swapchain,
        swapchain_images,
        swapchain_format,
        swapchain_extent,
        resource_image,
        sampler,
        pipeline,
        &frame_resources,
        sampler.snapshot.sampled_array_layer,
        0,
        false,
        None,
        None,
        None,
        None,
        0,
        "single-frame-present",
        0,
        "unpaced-single-frame-smoke",
        present_timing,
        vk::Semaphore::null(),
        0,
        None,
        None,
        clear_color,
        None,
    );
    native_vulkan_vulkanalia_destroy_decoded_image_present_frame_resources(device, frame_resources);
    result
}

#[allow(clippy::too_many_arguments)]
pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_present_decoded_image_frame(
    device: &Device,
    queue: vk::Queue,
    swapchain: vk::SwapchainKHR,
    swapchain_images: &[vk::Image],
    swapchain_format: vk::Format,
    swapchain_extent: vk::Extent2D,
    resource_image: &VulkanaliaVideoSessionResourceImage,
    sampler: &VulkanaliaDecodedImagePresentSamplerResources,
    pipeline: &VulkanaliaDecodedImagePresentPipelineResources,
    frame_resources: &VulkanaliaDecodedImagePresentFrameResources,
    sampled_array_layer: u32,
    present_frame_index: u32,
    present_frame_slot_prepared: bool,
    source_frame_pts_ns: Option<u64>,
    source_frame_duration_ns: Option<u64>,
    source_frame_pts_ms: Option<u64>,
    source_frame_duration_ms: Option<u64>,
    display_order_key: i64,
    display_order_key_source: &'static str,
    pacing_sleep_micros: u64,
    pacing_clock_model: &'static str,
    present_timing: VulkanaliaDecodedImagePresentTimingConfig,
    decode_complete_semaphore: vk::Semaphore,
    decode_complete_value: u64,
    queue_host_access_lock: Option<&Mutex<()>>,
    mut after_render_submit_before_present: Option<&mut dyn FnMut(u32) -> Result<(), String>>,
    clear_color: NativeVulkanClearColor,
    scene_overlay_draw: Option<VulkanaliaSceneVideoOverlayFrameDraw<'_>>,
) -> Result<NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot, String> {
    if swapchain_images.is_empty() {
        return Err("decoded image present requires at least one swapchain image".to_owned());
    }
    if swapchain_extent.width == 0 || swapchain_extent.height == 0 {
        return Err("decoded image present requires non-zero swapchain extent".to_owned());
    }
    if sampled_array_layer >= resource_image.snapshot.array_layers {
        return Err(format!(
            "decoded image present sampled layer {sampled_array_layer} exceeds {} image layers",
            resource_image.snapshot.array_layers
        ));
    }
    if frame_resources.swapchain_image_views.len() != swapchain_images.len() {
        return Err(format!(
            "decoded image present frame resource image-view count {} does not match swapchain image count {}",
            frame_resources.swapchain_image_views.len(),
            swapchain_images.len()
        ));
    }
    let frame_slot_count = frame_resources.in_flight.len();
    if frame_slot_count == 0
        || frame_resources.image_available.len() != frame_slot_count
        || frame_resources.render_finished.len() != frame_slot_count
    {
        return Err(format!(
            "decoded image present frame slots are inconsistent: image_available={}, render_finished={}, in_flight={}",
            frame_resources.image_available.len(),
            frame_resources.render_finished.len(),
            frame_resources.in_flight.len()
        ));
    }
    let present_frame_slot = present_frame_index as usize % frame_slot_count;
    let image_available = frame_resources.image_available[present_frame_slot];
    let in_flight = frame_resources.in_flight[present_frame_slot];
    let present_call_started_at = Instant::now();

    let mut present_wait_frame_slot_micros = if present_frame_slot_prepared {
        0
    } else {
        native_vulkan_vulkanalia_prepare_decoded_image_present_frame_slot(
            device,
            frame_resources,
            present_frame_slot as u32,
        )?
    };
    {
        let mut swapchain_image_in_flight = frame_resources
            .swapchain_image_in_flight
            .lock()
            .map_err(|_| {
                "decoded image present swapchain-image fence cache is poisoned".to_owned()
            })?;
        for cached_fence in swapchain_image_in_flight.iter_mut() {
            if *cached_fence == in_flight {
                *cached_fence = vk::Fence::null();
            }
        }
    }
    let stage_started_at = Instant::now();
    let (image_index, _) = unsafe {
        device.acquire_next_image_khr(swapchain, u64::MAX, image_available, vk::Fence::null())
    }
    .map_err(|err| format!("vkAcquireNextImageKHR(vulkanalia decoded image present): {err:?}"))?;
    let present_acquire_next_image_micros =
        native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
    let image_index_usize = image_index as usize;
    let render_finished = frame_resources
        .render_finished
        .get(image_index_usize)
        .copied()
        .ok_or_else(|| {
            format!("swapchain image index {image_index_usize} has no present semaphore")
        })?;
    let previous_swapchain_image_fence = {
        let swapchain_image_in_flight =
            frame_resources
                .swapchain_image_in_flight
                .lock()
                .map_err(|_| {
                    "decoded image present swapchain-image fence cache is poisoned".to_owned()
                })?;
        swapchain_image_in_flight
            .get(image_index_usize)
            .copied()
            .ok_or_else(|| {
                format!("swapchain image index {image_index_usize} has no tracked fence")
            })?
    };
    if previous_swapchain_image_fence != vk::Fence::null()
        && previous_swapchain_image_fence != in_flight
    {
        let stage_started_at = Instant::now();
        unsafe {
            device
                .wait_for_fences(&[previous_swapchain_image_fence], true, u64::MAX)
                .map_err(|err| {
                    format!(
                        "vkWaitForFences(vulkanalia decoded image present swapchain image reuse): {err:?}"
                    )
                })?;
        }
        present_wait_frame_slot_micros = present_wait_frame_slot_micros
            .saturating_add(native_vulkan_vulkanalia_elapsed_micros(stage_started_at));
    }
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

    let stage_started_at = Instant::now();
    native_vulkan_vulkanalia_record_decoded_image_present_command_buffer(
        device,
        command_buffer,
        swapchain_image,
        swapchain_view,
        swapchain_extent,
        resource_image.image,
        sampled_array_layer,
        &sampler.descriptor_heap,
        pipeline,
        clear_color,
        scene_overlay_draw,
    )?;
    let present_record_command_buffer_micros =
        native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
    let stage_started_at = Instant::now();
    let queue_host_access_guard =
        if let Some(lock) = queue_host_access_lock {
            Some(lock.lock().map_err(|_| {
                "decoded image present queue host-access lock is poisoned".to_owned()
            })?)
        } else {
            None
        };
    native_vulkan_vulkanalia_submit_decoded_image_present_command_buffer2(
        device,
        queue,
        command_buffer,
        image_available,
        render_finished,
        in_flight,
        decode_complete_semaphore,
        decode_complete_value,
    )?;
    {
        let mut swapchain_image_in_flight = frame_resources
            .swapchain_image_in_flight
            .lock()
            .map_err(|_| {
                "decoded image present swapchain-image fence cache is poisoned".to_owned()
            })?;
        let slot = swapchain_image_in_flight
            .get_mut(image_index_usize)
            .ok_or_else(|| {
                format!("swapchain image index {image_index_usize} has no tracked fence")
            })?;
        *slot = in_flight;
    }
    let present_submit_command_buffer_micros =
        native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
    // FFmpeg/libplacebo unmaps the AVFrame immediately after the rendered
    // frame is submitted/swapped, not after the next FIFO pacing wait
    // (references/ffmpeg/fftools/ffplay_renderer.c:780-786).
    let after_render_submit_before_present_result =
        if let Some(after_render_submit_before_present) =
            after_render_submit_before_present.as_deref_mut()
        {
            after_render_submit_before_present(present_frame_slot as u32)
        } else {
            Ok(())
        };

    let swapchains = [swapchain];
    let image_indices = [image_index];
    let wait_semaphores = [render_finished];
    let present_id = present_timing.present_id(present_frame_index);
    let present_ids = [present_id.unwrap_or(0)];
    let mut present_id2_info = present_id.map(|_| {
        vk::PresentId2KHR::builder()
            .present_ids(&present_ids)
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
    let stage_started_at = Instant::now();
    unsafe {
        device
            .queue_present_khr(queue, &present_info)
            .map_err(|err| {
                format!("vkQueuePresentKHR(vulkanalia decoded image present): {err:?}")
            })?;
    }
    drop(queue_host_access_guard);
    let present_queue_present_micros = native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
    after_render_submit_before_present_result?;
    let stage_started_at = Instant::now();
    let present_wait_after_present = present_timing.wait_after_queue_present(
        device,
        swapchain,
        present_id,
        "decoded image present",
    )?;
    let present_wait_after_queue_present_micros =
        native_vulkan_vulkanalia_elapsed_micros(stage_started_at);
    let present_call_total_micros =
        native_vulkan_vulkanalia_elapsed_micros(present_call_started_at);
    let scene_video_layer_draw_enabled =
        scene_overlay_draw.is_some_and(|draw| draw.video_draw.is_some());
    let scene_overlay_blend_draw_enabled =
        scene_overlay_draw.is_some_and(|draw| draw.overlay_draw.is_some());

    Ok(NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot {
        binding: "vulkanalia",
        route: "decoded-image-dynamic-rendering-present-draw",
        present_frame_index,
        sampled_array_layer,
        sampled_array_layer_source: "submitted-dst-base-array-layer-via-draw-first-instance",
        source_frame_pts_ns,
        source_frame_duration_ns,
        source_frame_pts_ms,
        source_frame_duration_ms,
        display_order_key,
        display_order_key_source,
        pacing_sleep_micros,
        pacing_clock_model,
        present_call_total_micros,
        present_wait_frame_slot_micros,
        present_acquire_next_image_micros,
        present_record_command_buffer_micros,
        present_submit_command_buffer_micros,
        present_queue_present_micros,
        present_wait_after_queue_present_micros,
        present_frame_slot: present_frame_slot as u32,
        present_sync_model: "frame-slot semaphore/fence reuse; no per-present queue_wait_idle",
        wait_idle_after_present: false,
        present_id,
        present_id_mode: present_timing.present_id_mode(),
        uses_present_id2: present_timing.present_id2_enabled,
        present_wait2_available: present_timing.present_wait2_enabled,
        present_wait_after_present,
        swapchain_image_index: image_index,
        swapchain_image_view_count: frame_resources.swapchain_image_views.len(),
        target_format: format!("{swapchain_format:?}"),
        extent: (swapchain_extent.width, swapchain_extent.height),
        clear_color: [clear_color.r, clear_color.g, clear_color.b, clear_color.a],
        command_buffer_recorded: true,
        submitted: true,
        presented: true,
        decoded_image_layout_transition: "video-decode-dpb -> shader-read-only-optimal -> video-decode-dpb",
        swapchain_layout_transition: "undefined -> color-attachment-optimal -> present-src-khr",
        render_model: if scene_video_layer_draw_enabled {
            "VK_EXT_descriptor_heap retained Y/UV plane-array sampler mapping plus native scene video layer indexed quads -> Vulkan 1.4 dynamic rendering pass -> Wayland swapchain"
        } else if scene_overlay_blend_draw_enabled {
            "VK_EXT_descriptor_heap retained Y/UV plane-array sampler mapping plus native scene overlay draw -> Vulkan 1.4 dynamic rendering pass -> Wayland swapchain"
        } else {
            "VK_EXT_descriptor_heap retained Y/UV plane-array sampler mapping -> Vulkan 1.3/1.4 dynamic rendering fullscreen triangle -> Wayland swapchain"
        },
        command_order: native_vulkan_vulkanalia_decoded_image_present_command_order(
            true,
            present_timing.present_id_mode(),
            present_timing.present_wait_mode(),
            scene_video_layer_draw_enabled,
            scene_overlay_blend_draw_enabled,
        ),
        uses_pipeline_rendering_create_info: true,
        uses_dynamic_rendering: true,
        uses_synchronization2: true,
        uses_submit2: true,
        zero_copy_presented: true,
        descriptor_model: "VK_EXT_descriptor_heap",
        ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
    })
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_wait_decoded_image_present_frame_slot(
    device: &Device,
    resources: &VulkanaliaDecodedImagePresentFrameResources,
    present_frame_slot: u32,
) -> Result<u64, String> {
    let slot = present_frame_slot as usize;
    let fence = resources.in_flight.get(slot).copied().ok_or_else(|| {
        format!(
            "decoded image present frame slot {slot} exceeds {} in-flight fence(s)",
            resources.in_flight.len()
        )
    })?;
    let started_at = Instant::now();
    unsafe {
        device
            .wait_for_fences(&[fence], true, u64::MAX)
            .map_err(|err| {
                format!("vkWaitForFences(vulkanalia decoded image present layer release): {err:?}")
            })?;
    }
    Ok(native_vulkan_vulkanalia_elapsed_micros(started_at))
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_try_complete_decoded_image_present_frame_slot(
    device: &Device,
    resources: &VulkanaliaDecodedImagePresentFrameResources,
    present_frame_slot: u32,
) -> Result<bool, String> {
    let slot = present_frame_slot as usize;
    let fence = resources.in_flight.get(slot).copied().ok_or_else(|| {
        format!(
            "decoded image present frame slot {slot} exceeds {} in-flight fence(s)",
            resources.in_flight.len()
        )
    })?;
    let status = unsafe { device.get_fence_status(fence) }.map_err(|err| {
        format!("vkGetFenceStatus(vulkanalia decoded image present layer release): {err:?}")
    })?;
    if status == vk::SuccessCode::SUCCESS {
        Ok(true)
    } else if status == vk::SuccessCode::NOT_READY {
        Ok(false)
    } else {
        Err(format!(
            "vkGetFenceStatus(vulkanalia decoded image present layer release) returned unexpected status {status:?}"
        ))
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_decoded_image_present_frame_slot_count(
    resources: &VulkanaliaDecodedImagePresentFrameResources,
) -> usize {
    resources.in_flight.len()
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_decoded_image_present_command_pool(
    resources: &VulkanaliaDecodedImagePresentFrameResources,
) -> vk::CommandPool {
    resources.command_pool
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_prepare_decoded_image_present_frame_slot(
    device: &Device,
    resources: &VulkanaliaDecodedImagePresentFrameResources,
    present_frame_slot: u32,
) -> Result<u64, String> {
    let slot = present_frame_slot as usize;
    let fence = resources.in_flight.get(slot).copied().ok_or_else(|| {
        format!(
            "decoded image present frame slot {slot} exceeds {} in-flight fence(s)",
            resources.in_flight.len()
        )
    })?;
    let started_at = Instant::now();
    unsafe {
        device
            .wait_for_fences(&[fence], true, u64::MAX)
            .map_err(|err| format!("vkWaitForFences(vulkanalia decoded image present): {err:?}"))?;
        device
            .reset_fences(&[fence])
            .map_err(|err| format!("vkResetFences(vulkanalia decoded image present): {err:?}"))?;
    }
    Ok(native_vulkan_vulkanalia_elapsed_micros(started_at))
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_destroy_decoded_image_present_frame_resources(
    device: &Device,
    resources: VulkanaliaDecodedImagePresentFrameResources,
) {
    let _ = unsafe { device.device_wait_idle() };
    native_vulkan_vulkanalia_destroy_partial_decoded_image_present_frame_resources(
        device,
        resources.swapchain_image_views,
        resources.command_pool,
        resources.image_available,
        resources.render_finished,
        resources.in_flight,
        resources.decode_complete,
    );
}

fn native_vulkan_vulkanalia_destroy_partial_decoded_image_present_frame_resources(
    device: &Device,
    swapchain_image_views: Vec<vk::ImageView>,
    command_pool: vk::CommandPool,
    image_available: Vec<vk::Semaphore>,
    render_finished: Vec<vk::Semaphore>,
    in_flight: Vec<vk::Fence>,
    decode_complete: vk::Semaphore,
) {
    unsafe {
        if decode_complete != vk::Semaphore::null() {
            device.destroy_semaphore(decode_complete, None);
        }
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

fn native_vulkan_vulkanalia_create_present_swapchain_image_views(
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
            .subresource_range(native_vulkan_vulkanalia_color_subresource_range());
        match unsafe { device.create_image_view(&create_info, None) } {
            Ok(view) => views.push(view),
            Err(err) => {
                for view in views {
                    unsafe {
                        device.destroy_image_view(view, None);
                    }
                }
                return Err(format!(
                    "vkCreateImageView(vulkanalia decoded image present swapchain): {err:?}"
                ));
            }
        }
    }
    Ok(views)
}

#[allow(clippy::too_many_arguments)]
fn native_vulkan_vulkanalia_record_decoded_image_present_command_buffer(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    swapchain_image: vk::Image,
    swapchain_view: vk::ImageView,
    extent: vk::Extent2D,
    decoded_image: vk::Image,
    sampled_array_layer: u32,
    descriptor_heap: &VulkanaliaDescriptorHeapImageSamplerResources,
    pipeline: &VulkanaliaDecodedImagePresentPipelineResources,
    clear_color: NativeVulkanClearColor,
    scene_overlay_draw: Option<VulkanaliaSceneVideoOverlayFrameDraw<'_>>,
) -> Result<(), String> {
    unsafe {
        device
            .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
            .map_err(|err| {
                format!("vkResetCommandBuffer(vulkanalia decoded image present): {err:?}")
            })?;
        let begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
            .build();
        device
            .begin_command_buffer(command_buffer, &begin_info)
            .map_err(|err| {
                format!("vkBeginCommandBuffer(vulkanalia decoded image present): {err:?}")
            })?;

        let decoded_to_shader = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::NONE)
            .src_access_mask(vk::AccessFlags2::NONE)
            .dst_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
            .dst_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
            .old_layout(vk::ImageLayout::VIDEO_DECODE_DPB_KHR)
            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(decoded_image)
            .subresource_range(
                native_vulkan_vulkanalia_decoded_image_layer_subresource_range(sampled_array_layer),
            )
            .build();
        let swapchain_to_attachment = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE)
            .src_access_mask(vk::AccessFlags2::empty())
            .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(swapchain_image)
            .subresource_range(native_vulkan_vulkanalia_color_subresource_range())
            .build();
        let image_barriers = [decoded_to_shader, swapchain_to_attachment];
        let dependency = vk::DependencyInfo::builder()
            .image_memory_barriers(&image_barriers)
            .build();
        device.cmd_pipeline_barrier2(command_buffer, &dependency);

        let clear_value = vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [clear_color.r, clear_color.g, clear_color.b, clear_color.a],
            },
        };
        let color_attachment = vk::RenderingAttachmentInfo::builder()
            .image_view(swapchain_view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(clear_value)
            .build();
        let color_attachments = [color_attachment];
        let render_area = vk::Rect2D::builder()
            .offset(vk::Offset2D { x: 0, y: 0 })
            .extent(extent)
            .build();
        let rendering_info = vk::RenderingInfo::builder()
            .render_area(render_area)
            .layer_count(1)
            .color_attachments(&color_attachments)
            .build();
        device.cmd_begin_rendering(command_buffer, &rendering_info);
        let resource_bind =
            native_vulkan_vulkanalia_descriptor_heap_resource_bind_info(descriptor_heap);
        let sampler_bind =
            native_vulkan_vulkanalia_descriptor_heap_sampler_bind_info(descriptor_heap);
        device.cmd_bind_resource_heap_ext(command_buffer, &resource_bind);
        device.cmd_bind_sampler_heap_ext(command_buffer, &sampler_bind);
        let scene_video_layer_draw = scene_overlay_draw.and_then(|draw| draw.video_draw);
        if let Some(scene_video_layer_draw) = scene_video_layer_draw {
            native_vulkan_vulkanalia_record_decoded_image_scene_video_layer_draws_inside_rendering(
                device,
                command_buffer,
                extent,
                &pipeline.scene_video_layer,
                scene_video_layer_draw,
                sampled_array_layer,
            )?;
        } else {
            device.cmd_bind_pipeline(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                pipeline.pipeline,
            );
            device.cmd_draw(command_buffer, 3, 1, 0, sampled_array_layer);
        }
        if let Some(scene_overlay_draw) = scene_overlay_draw {
            native_vulkan_vulkanalia_record_scene_video_overlay_draws_inside_rendering(
                device,
                command_buffer,
                extent,
                scene_overlay_draw,
            )?;
        }
        device.cmd_end_rendering(command_buffer);

        let decoded_to_decode = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
            .src_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
            .dst_stage_mask(vk::PipelineStageFlags2::NONE)
            .dst_access_mask(vk::AccessFlags2::NONE)
            .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .new_layout(vk::ImageLayout::VIDEO_DECODE_DPB_KHR)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(decoded_image)
            .subresource_range(
                native_vulkan_vulkanalia_decoded_image_layer_subresource_range(sampled_array_layer),
            )
            .build();
        let swapchain_to_present = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
            .dst_stage_mask(vk::PipelineStageFlags2::BOTTOM_OF_PIPE)
            .dst_access_mask(vk::AccessFlags2::empty())
            .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
            .image(swapchain_image)
            .subresource_range(native_vulkan_vulkanalia_color_subresource_range())
            .build();
        let present_barriers = [decoded_to_decode, swapchain_to_present];
        let present_dependency = vk::DependencyInfo::builder()
            .image_memory_barriers(&present_barriers)
            .build();
        device.cmd_pipeline_barrier2(command_buffer, &present_dependency);

        device.end_command_buffer(command_buffer).map_err(|err| {
            format!("vkEndCommandBuffer(vulkanalia decoded image present): {err:?}")
        })?;
    }

    Ok(())
}

fn native_vulkan_vulkanalia_record_decoded_image_scene_video_layer_draws_inside_rendering(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    extent: vk::Extent2D,
    pipeline: &VulkanaliaDecodedImageSceneVideoLayerPipelineResources,
    draw: VulkanaliaSceneVideoLayerFrameDraw<'_>,
    sampled_array_layer: u32,
) -> Result<u32, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("decoded scene video layer draw requires non-zero extent".to_owned());
    }
    if draw.draw_commands.is_empty() {
        return Err("decoded scene video layer draw requires at least one draw".to_owned());
    }
    for draw_command in draw.draw_commands {
        if draw_command.index_count == 0 {
            return Err("decoded scene video layer draw requires non-empty indices".to_owned());
        }
    }

    unsafe {
        device.cmd_bind_pipeline(
            command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            pipeline.pipeline,
        );
        let vertex_buffers = [draw.vertex_buffer];
        let vertex_offsets = [0u64];
        device.cmd_bind_vertex_buffers(command_buffer, 0, &vertex_buffers, &vertex_offsets);
        device.cmd_bind_index_buffer(command_buffer, draw.index_buffer, 0, vk::IndexType::UINT32);
        let push_constants = [extent.width as f32, extent.height as f32];
        let push_constant_bytes = std::slice::from_raw_parts(
            push_constants.as_ptr().cast::<u8>(),
            DECODED_IMAGE_SCENE_VIDEO_LAYER_PUSH_CONSTANT_BYTES as usize,
        );
        device.cmd_push_constants(
            command_buffer,
            pipeline.pipeline_layout,
            vk::ShaderStageFlags::VERTEX,
            0,
            push_constant_bytes,
        );
        for draw_command in draw.draw_commands {
            device.cmd_draw_indexed(
                command_buffer,
                draw_command.index_count,
                1,
                draw_command.first_index,
                0,
                sampled_array_layer,
            );
        }
    }

    Ok(draw
        .draw_commands
        .iter()
        .fold(0u32, |sum, draw| sum.saturating_add(draw.index_count)))
}

fn native_vulkan_vulkanalia_submit_decoded_image_present_command_buffer2(
    device: &Device,
    queue: vk::Queue,
    command_buffer: vk::CommandBuffer,
    image_available: vk::Semaphore,
    render_finished: vk::Semaphore,
    fence: vk::Fence,
    decode_complete_semaphore: vk::Semaphore,
    decode_complete_value: u64,
) -> Result<(), String> {
    // Wait for the swapchain image at color output. FFmpeg mirrors AVVkFrame
    // semaphore values as frame dependencies (references/ffmpeg/libavcodec/
    // vulkan_decode.c:575-586); the decode submit signals at video-decode
    // completion, while this present submit waits before any graphics command
    // mutates the decoded image layout.
    let image_available_wait = vk::SemaphoreSubmitInfo::builder()
        .semaphore(image_available)
        .stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        .build();
    let decode_complete_wait = vk::SemaphoreSubmitInfo::builder()
        .semaphore(decode_complete_semaphore)
        .value(decode_complete_value)
        .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .build();
    let waits_with_decode = [image_available_wait, decode_complete_wait];
    let waits_without_decode = [image_available_wait];
    let command_buffer_info = vk::CommandBufferSubmitInfo::builder()
        .command_buffer(command_buffer)
        .build();
    let command_buffer_infos = [command_buffer_info];
    let signal = vk::SemaphoreSubmitInfo::builder()
        .semaphore(render_finished)
        .stage_mask(vk::PipelineStageFlags2::ALL_COMMANDS)
        .build();
    let signals = [signal];
    let mut submit_builder = vk::SubmitInfo2::builder()
        .command_buffer_infos(&command_buffer_infos)
        .signal_semaphore_infos(&signals);
    if decode_complete_semaphore != vk::Semaphore::null() {
        submit_builder = submit_builder.wait_semaphore_infos(&waits_with_decode);
    } else {
        submit_builder = submit_builder.wait_semaphore_infos(&waits_without_decode);
    }
    let submit_info = submit_builder.build();

    unsafe {
        device
            .queue_submit2(queue, &[submit_info], fence)
            .map_err(|err| format!("vkQueueSubmit2(vulkanalia decoded image present): {err:?}"))?;
    }

    Ok(())
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_decoded_image_present_command_order(
    same_queue_family: bool,
    present_id_mode: &'static str,
    present_wait_mode: &'static str,
    scene_video_layer_draw_enabled: bool,
    scene_overlay_draw_enabled: bool,
) -> Vec<&'static str> {
    let fullscreen_bind_steps = [
        "cmd_bind_resource_heap_ext",
        "cmd_bind_sampler_heap_ext",
        "draw_with_descriptor_heap_plane_array_sampler_mapping",
    ];
    let video_layer_bind_steps = [
        "cmd_bind_resource_heap_ext",
        "cmd_bind_sampler_heap_ext",
        "cmd_bind_scene_video_layer_pipeline",
        "cmd_draw_scene_video_layers_inside_video_rendering",
    ];
    let mut order = if same_queue_family {
        let mut order = vec![
            "queue_submit2_decode",
            "cmd_pipeline_barrier2_shader_read",
            "cmd_begin_rendering",
        ];
        if scene_video_layer_draw_enabled {
            order.extend(video_layer_bind_steps);
        } else {
            order.extend(fullscreen_bind_steps);
        }
        if scene_overlay_draw_enabled {
            order.extend([
                "cmd_bind_scene_overlay_pipeline",
                "cmd_draw_scene_overlay_inside_video_rendering",
            ]);
        }
        order.extend([
            "cmd_end_rendering",
            "cmd_pipeline_barrier2_decoded_restore",
            "cmd_pipeline_barrier2_present",
            "queue_submit2_present",
            "defer_frame_slot_reuse_until_render_fence",
            "queue_present_khr",
            "no_queue_wait_idle_after_present",
        ]);
        order
    } else {
        let mut order = vec![
            "queue_submit2_decode",
            "cmd_pipeline_barrier2_video_release",
            "cmd_pipeline_barrier2_graphics_acquire_shader_read",
            "cmd_begin_rendering",
        ];
        if scene_video_layer_draw_enabled {
            order.extend(video_layer_bind_steps);
        } else {
            order.extend(fullscreen_bind_steps);
        }
        if scene_overlay_draw_enabled {
            order.extend([
                "cmd_bind_scene_overlay_pipeline",
                "cmd_draw_scene_overlay_inside_video_rendering",
            ]);
        }
        order.extend([
            "cmd_end_rendering",
            "cmd_pipeline_barrier2_decoded_restore",
            "cmd_pipeline_barrier2_present",
            "queue_submit2_present",
            "defer_frame_slot_reuse_until_render_fence",
            "queue_present_khr",
            "no_queue_wait_idle_after_present",
        ]);
        order
    };
    match present_id_mode {
        "present-id2-khr" => order.insert(order.len().saturating_sub(2), "present_id2_khr"),
        _ => {}
    }
    match present_wait_mode {
        "present-wait2-khr" => order.insert(order.len().saturating_sub(1), "wait_for_present2_khr"),
        _ => {}
    }
    order
}

fn native_vulkan_vulkanalia_create_shader_module(
    device: &Device,
    code: &[u32],
    label: &'static str,
) -> Result<vk::ShaderModule, String> {
    if code.first().copied() != Some(0x0723_0203) {
        return Err(format!(
            "decoded present {label} shader is not valid SPIR-V bytecode"
        ));
    }
    let create_info = vk::ShaderModuleCreateInfo::builder()
        .code(code)
        .code_size(native_vulkan_vulkanalia_shader_code_size_bytes(code));
    unsafe { device.create_shader_module(&create_info, None) }
        .map_err(|err| format!("vkCreateShaderModule(vulkanalia {label}): {err:?}"))
}

fn native_vulkan_vulkanalia_shader_code_size_bytes(code: &[u32]) -> usize {
    std::mem::size_of_val(code)
}

fn native_vulkan_vulkanalia_color_subresource_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1)
        .build()
}

fn native_vulkan_vulkanalia_decoded_image_layer_subresource_range(
    sampled_array_layer: u32,
) -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(sampled_array_layer)
        .layer_count(1)
        .build()
}

const NATIVE_VULKAN_VULKANALIA_PLANE_PRESENT_VERTEX_SPIRV: [u32; 312] = [
    0x07230203, 0x00010000, 0x000d000b, 0x00000038, 0x00000000, 0x00020011, 0x00000001, 0x0006000b,
    0x00000001, 0x4c534c47, 0x6474732e, 0x3035342e, 0x00000000, 0x0003000e, 0x00000000, 0x00000001,
    0x000a000f, 0x00000000, 0x00000004, 0x6e69616d, 0x00000000, 0x0000001f, 0x00000023, 0x0000002f,
    0x00000034, 0x00000035, 0x00030047, 0x0000001d, 0x00000002, 0x00050048, 0x0000001d, 0x00000000,
    0x0000000b, 0x00000000, 0x00050048, 0x0000001d, 0x00000001, 0x0000000b, 0x00000001, 0x00050048,
    0x0000001d, 0x00000002, 0x0000000b, 0x00000003, 0x00050048, 0x0000001d, 0x00000003, 0x0000000b,
    0x00000004, 0x00040047, 0x00000023, 0x0000000b, 0x0000002a, 0x00040047, 0x0000002f, 0x0000001e,
    0x00000000, 0x00030047, 0x00000034, 0x0000000e, 0x00040047, 0x00000034, 0x0000001e, 0x00000001,
    0x00040047, 0x00000035, 0x0000000b, 0x0000002b, 0x00020013, 0x00000002, 0x00030021, 0x00000003,
    0x00000002, 0x00030016, 0x00000006, 0x00000020, 0x00040017, 0x00000007, 0x00000006, 0x00000002,
    0x00040015, 0x00000008, 0x00000020, 0x00000000, 0x0004002b, 0x00000008, 0x00000009, 0x00000003,
    0x0004001c, 0x0000000a, 0x00000007, 0x00000009, 0x00040020, 0x0000000b, 0x00000007, 0x0000000a,
    0x0004002b, 0x00000006, 0x0000000d, 0xbf800000, 0x0005002c, 0x00000007, 0x0000000e, 0x0000000d,
    0x0000000d, 0x0004002b, 0x00000006, 0x0000000f, 0x40400000, 0x0005002c, 0x00000007, 0x00000010,
    0x0000000f, 0x0000000d, 0x0005002c, 0x00000007, 0x00000011, 0x0000000d, 0x0000000f, 0x0006002c,
    0x0000000a, 0x00000012, 0x0000000e, 0x00000010, 0x00000011, 0x0004002b, 0x00000006, 0x00000014,
    0x00000000, 0x0005002c, 0x00000007, 0x00000015, 0x00000014, 0x00000014, 0x0004002b, 0x00000006,
    0x00000016, 0x40000000, 0x0005002c, 0x00000007, 0x00000017, 0x00000016, 0x00000014, 0x0005002c,
    0x00000007, 0x00000018, 0x00000014, 0x00000016, 0x0006002c, 0x0000000a, 0x00000019, 0x00000015,
    0x00000017, 0x00000018, 0x00040017, 0x0000001a, 0x00000006, 0x00000004, 0x0004002b, 0x00000008,
    0x0000001b, 0x00000001, 0x0004001c, 0x0000001c, 0x00000006, 0x0000001b, 0x0006001e, 0x0000001d,
    0x0000001a, 0x00000006, 0x0000001c, 0x0000001c, 0x00040020, 0x0000001e, 0x00000003, 0x0000001d,
    0x0004003b, 0x0000001e, 0x0000001f, 0x00000003, 0x00040015, 0x00000020, 0x00000020, 0x00000001,
    0x0004002b, 0x00000020, 0x00000021, 0x00000000, 0x00040020, 0x00000022, 0x00000001, 0x00000020,
    0x0004003b, 0x00000022, 0x00000023, 0x00000001, 0x00040020, 0x00000025, 0x00000007, 0x00000007,
    0x0004002b, 0x00000006, 0x00000028, 0x3f800000, 0x00040020, 0x0000002c, 0x00000003, 0x0000001a,
    0x00040020, 0x0000002e, 0x00000003, 0x00000007, 0x0004003b, 0x0000002e, 0x0000002f, 0x00000003,
    0x00040020, 0x00000033, 0x00000003, 0x00000008, 0x0004003b, 0x00000033, 0x00000034, 0x00000003,
    0x0004003b, 0x00000022, 0x00000035, 0x00000001, 0x00050036, 0x00000002, 0x00000004, 0x00000000,
    0x00000003, 0x000200f8, 0x00000005, 0x0004003b, 0x0000000b, 0x0000000c, 0x00000007, 0x0004003b,
    0x0000000b, 0x00000013, 0x00000007, 0x0003003e, 0x0000000c, 0x00000012, 0x0003003e, 0x00000013,
    0x00000019, 0x0004003d, 0x00000020, 0x00000024, 0x00000023, 0x00050041, 0x00000025, 0x00000026,
    0x0000000c, 0x00000024, 0x0004003d, 0x00000007, 0x00000027, 0x00000026, 0x00050051, 0x00000006,
    0x00000029, 0x00000027, 0x00000000, 0x00050051, 0x00000006, 0x0000002a, 0x00000027, 0x00000001,
    0x00070050, 0x0000001a, 0x0000002b, 0x00000029, 0x0000002a, 0x00000014, 0x00000028, 0x00050041,
    0x0000002c, 0x0000002d, 0x0000001f, 0x00000021, 0x0003003e, 0x0000002d, 0x0000002b, 0x00050041,
    0x00000025, 0x00000031, 0x00000013, 0x00000024, 0x0004003d, 0x00000007, 0x00000032, 0x00000031,
    0x0003003e, 0x0000002f, 0x00000032, 0x0004003d, 0x00000020, 0x00000036, 0x00000035, 0x0004007c,
    0x00000008, 0x00000037, 0x00000036, 0x0003003e, 0x00000034, 0x00000037, 0x000100fd, 0x00010038,
];

const NATIVE_VULKAN_VULKANALIA_PLANE_PRESENT_FRAGMENT_SPIRV: [u32; 291] = [
    0x07230203, 0x00010000, 0x000d000b, 0x0000004e, 0x00000000, 0x00020011, 0x00000001, 0x0006000b,
    0x00000001, 0x4c534c47, 0x6474732e, 0x3035342e, 0x00000000, 0x0003000e, 0x00000000, 0x00000001,
    0x0008000f, 0x00000004, 0x00000004, 0x6e69616d, 0x00000000, 0x0000000c, 0x00000010, 0x00000048,
    0x00030010, 0x00000004, 0x00000007, 0x00040047, 0x0000000c, 0x0000001e, 0x00000000, 0x00030047,
    0x00000010, 0x0000000e, 0x00040047, 0x00000010, 0x0000001e, 0x00000001, 0x00040047, 0x0000001b,
    0x00000021, 0x00000000, 0x00040047, 0x0000001b, 0x00000022, 0x00000000, 0x00040047, 0x00000024,
    0x00000021, 0x00000001, 0x00040047, 0x00000024, 0x00000022, 0x00000000, 0x00040047, 0x00000048,
    0x0000001e, 0x00000000, 0x00020013, 0x00000002, 0x00030021, 0x00000003, 0x00000002, 0x00030016,
    0x00000006, 0x00000020, 0x00040017, 0x00000007, 0x00000006, 0x00000003, 0x00040017, 0x0000000a,
    0x00000006, 0x00000002, 0x00040020, 0x0000000b, 0x00000001, 0x0000000a, 0x0004003b, 0x0000000b,
    0x0000000c, 0x00000001, 0x00040015, 0x0000000e, 0x00000020, 0x00000000, 0x00040020, 0x0000000f,
    0x00000001, 0x0000000e, 0x0004003b, 0x0000000f, 0x00000010, 0x00000001, 0x00090019, 0x00000018,
    0x00000006, 0x00000001, 0x00000000, 0x00000001, 0x00000000, 0x00000001, 0x00000000, 0x0003001b,
    0x00000019, 0x00000018, 0x00040020, 0x0000001a, 0x00000000, 0x00000019, 0x0004003b, 0x0000001a,
    0x0000001b, 0x00000000, 0x00040017, 0x0000001e, 0x00000006, 0x00000004, 0x0004003b, 0x0000001a,
    0x00000024, 0x00000000, 0x0004002b, 0x00000006, 0x00000029, 0x3f000000, 0x0005002c, 0x0000000a,
    0x0000002a, 0x00000029, 0x00000029, 0x0004002b, 0x00000006, 0x0000002e, 0x3fb374bc, 0x0004002b,
    0x00000006, 0x00000036, 0x3eb03298, 0x0004002b, 0x00000006, 0x0000003b, 0x3f36d19e, 0x0004002b,
    0x00000006, 0x00000042, 0x3fe2d0e5, 0x00040020, 0x00000047, 0x00000003, 0x0000001e, 0x0004003b,
    0x00000047, 0x00000048, 0x00000003, 0x0004002b, 0x00000006, 0x0000004c, 0x3f800000, 0x00050036,
    0x00000002, 0x00000004, 0x00000000, 0x00000003, 0x000200f8, 0x00000005, 0x0004003d, 0x0000000a,
    0x0000000d, 0x0000000c, 0x0004003d, 0x0000000e, 0x00000011, 0x00000010, 0x00040070, 0x00000006,
    0x00000012, 0x00000011, 0x00050051, 0x00000006, 0x00000013, 0x0000000d, 0x00000000, 0x00050051,
    0x00000006, 0x00000014, 0x0000000d, 0x00000001, 0x00060050, 0x00000007, 0x00000015, 0x00000013,
    0x00000014, 0x00000012, 0x0004003d, 0x00000019, 0x0000001c, 0x0000001b, 0x00050057, 0x0000001e,
    0x0000001f, 0x0000001c, 0x00000015, 0x00050051, 0x00000006, 0x00000021, 0x0000001f, 0x00000000,
    0x0004003d, 0x00000019, 0x00000025, 0x00000024, 0x00050057, 0x0000001e, 0x00000027, 0x00000025,
    0x00000015, 0x0007004f, 0x0000000a, 0x00000028, 0x00000027, 0x00000027, 0x00000000, 0x00000001,
    0x00050083, 0x0000000a, 0x0000002b, 0x00000028, 0x0000002a, 0x00050051, 0x00000006, 0x00000031,
    0x0000002b, 0x00000001, 0x00050085, 0x00000006, 0x00000032, 0x0000002e, 0x00000031, 0x00050081,
    0x00000006, 0x00000033, 0x00000021, 0x00000032, 0x00050051, 0x00000006, 0x00000038, 0x0000002b,
    0x00000000, 0x00050085, 0x00000006, 0x00000039, 0x00000036, 0x00000038, 0x00050083, 0x00000006,
    0x0000003a, 0x00000021, 0x00000039, 0x00050085, 0x00000006, 0x0000003e, 0x0000003b, 0x00000031,
    0x00050083, 0x00000006, 0x0000003f, 0x0000003a, 0x0000003e, 0x00050085, 0x00000006, 0x00000045,
    0x00000042, 0x00000038, 0x00050081, 0x00000006, 0x00000046, 0x00000021, 0x00000045, 0x00070050,
    0x0000001e, 0x0000004d, 0x00000033, 0x0000003f, 0x00000046, 0x0000004c, 0x0003003e, 0x00000048,
    0x0000004d, 0x000100fd, 0x00010038,
];

const NATIVE_VULKAN_VULKANALIA_PLANE_SCENE_VIDEO_LAYER_VERTEX_SPIRV: [u32; 495] = [
    0x07230203, 0x00010000, 0x000d000b, 0x00000043, 0x00000000, 0x00020011, 0x00000001, 0x0006000b,
    0x00000001, 0x4c534c47, 0x6474732e, 0x3035342e, 0x00000000, 0x0003000e, 0x00000000, 0x00000001,
    0x000d000f, 0x00000000, 0x00000004, 0x6e69616d, 0x00000000, 0x0000000b, 0x0000002b, 0x00000036,
    0x00000037, 0x0000003a, 0x0000003b, 0x0000003e, 0x00000040, 0x00030003, 0x00000002, 0x000001c2,
    0x000a0004, 0x475f4c47, 0x4c474f4f, 0x70635f45, 0x74735f70, 0x5f656c79, 0x656e696c, 0x7269645f,
    0x69746365, 0x00006576, 0x00080004, 0x475f4c47, 0x4c474f4f, 0x6e695f45, 0x64756c63, 0x69645f65,
    0x74636572, 0x00657669, 0x00040005, 0x00000004, 0x6e69616d, 0x00000000, 0x00030005, 0x00000009,
    0x0063646e, 0x00040005, 0x0000000b, 0x705f6e69, 0x0000736f, 0x00040005, 0x00000011, 0x68737550,
    0x00000000, 0x00050006, 0x00000011, 0x00000000, 0x65747865, 0x0000746e, 0x00050005, 0x00000013,
    0x68737570, 0x7461645f, 0x00000061, 0x00060005, 0x00000029, 0x505f6c67, 0x65567265, 0x78657472,
    0x00000000, 0x00060006, 0x00000029, 0x00000000, 0x505f6c67, 0x7469736f, 0x006e6f69, 0x00070006,
    0x00000029, 0x00000001, 0x505f6c67, 0x746e696f, 0x657a6953, 0x00000000, 0x00070006, 0x00000029,
    0x00000002, 0x435f6c67, 0x4470696c, 0x61747369, 0x0065636e, 0x00070006, 0x00000029, 0x00000003,
    0x435f6c67, 0x446c6c75, 0x61747369, 0x0065636e, 0x00030005, 0x0000002b, 0x00000000, 0x00040005,
    0x00000036, 0x5f74756f, 0x00007675, 0x00040005, 0x00000037, 0x755f6e69, 0x00000076, 0x00050005,
    0x0000003a, 0x5f74756f, 0x6361706f, 0x00797469, 0x00050005, 0x0000003b, 0x6f5f6e69, 0x69636170,
    0x00007974, 0x00050005, 0x0000003e, 0x5f74756f, 0x6579616c, 0x00000072, 0x00070005, 0x00000040,
    0x495f6c67, 0x6174736e, 0x4965636e, 0x7865646e, 0x00000000, 0x00040047, 0x0000000b, 0x0000001e,
    0x00000000, 0x00030047, 0x00000011, 0x00000002, 0x00050048, 0x00000011, 0x00000000, 0x00000023,
    0x00000000, 0x00030047, 0x00000029, 0x00000002, 0x00050048, 0x00000029, 0x00000000, 0x0000000b,
    0x00000000, 0x00050048, 0x00000029, 0x00000001, 0x0000000b, 0x00000001, 0x00050048, 0x00000029,
    0x00000002, 0x0000000b, 0x00000003, 0x00050048, 0x00000029, 0x00000003, 0x0000000b, 0x00000004,
    0x00040047, 0x00000036, 0x0000001e, 0x00000000, 0x00040047, 0x00000037, 0x0000001e, 0x00000001,
    0x00040047, 0x0000003a, 0x0000001e, 0x00000001, 0x00040047, 0x0000003b, 0x0000001e, 0x00000002,
    0x00030047, 0x0000003e, 0x0000000e, 0x00040047, 0x0000003e, 0x0000001e, 0x00000002, 0x00040047,
    0x00000040, 0x0000000b, 0x0000002b, 0x00020013, 0x00000002, 0x00030021, 0x00000003, 0x00000002,
    0x00030016, 0x00000006, 0x00000020, 0x00040017, 0x00000007, 0x00000006, 0x00000002, 0x00040020,
    0x00000008, 0x00000007, 0x00000007, 0x00040020, 0x0000000a, 0x00000001, 0x00000007, 0x0004003b,
    0x0000000a, 0x0000000b, 0x00000001, 0x00040015, 0x0000000c, 0x00000020, 0x00000000, 0x0004002b,
    0x0000000c, 0x0000000d, 0x00000000, 0x00040020, 0x0000000e, 0x00000001, 0x00000006, 0x0003001e,
    0x00000011, 0x00000007, 0x00040020, 0x00000012, 0x00000009, 0x00000011, 0x0004003b, 0x00000012,
    0x00000013, 0x00000009, 0x00040015, 0x00000014, 0x00000020, 0x00000001, 0x0004002b, 0x00000014,
    0x00000015, 0x00000000, 0x00040020, 0x00000016, 0x00000009, 0x00000006, 0x0004002b, 0x00000006,
    0x0000001a, 0x40000000, 0x0004002b, 0x00000006, 0x0000001c, 0x3f800000, 0x0004002b, 0x0000000c,
    0x0000001e, 0x00000001, 0x00040017, 0x00000027, 0x00000006, 0x00000004, 0x0004001c, 0x00000028,
    0x00000006, 0x0000001e, 0x0006001e, 0x00000029, 0x00000027, 0x00000006, 0x00000028, 0x00000028,
    0x00040020, 0x0000002a, 0x00000003, 0x00000029, 0x0004003b, 0x0000002a, 0x0000002b, 0x00000003,
    0x00040020, 0x0000002c, 0x00000007, 0x00000006, 0x0004002b, 0x00000006, 0x00000031, 0x00000000,
    0x00040020, 0x00000033, 0x00000003, 0x00000027, 0x00040020, 0x00000035, 0x00000003, 0x00000007,
    0x0004003b, 0x00000035, 0x00000036, 0x00000003, 0x0004003b, 0x0000000a, 0x00000037, 0x00000001,
    0x00040020, 0x00000039, 0x00000003, 0x00000006, 0x0004003b, 0x00000039, 0x0000003a, 0x00000003,
    0x0004003b, 0x0000000e, 0x0000003b, 0x00000001, 0x00040020, 0x0000003d, 0x00000003, 0x0000000c,
    0x0004003b, 0x0000003d, 0x0000003e, 0x00000003, 0x00040020, 0x0000003f, 0x00000001, 0x00000014,
    0x0004003b, 0x0000003f, 0x00000040, 0x00000001, 0x00050036, 0x00000002, 0x00000004, 0x00000000,
    0x00000003, 0x000200f8, 0x00000005, 0x0004003b, 0x00000008, 0x00000009, 0x00000007, 0x00050041,
    0x0000000e, 0x0000000f, 0x0000000b, 0x0000000d, 0x0004003d, 0x00000006, 0x00000010, 0x0000000f,
    0x00060041, 0x00000016, 0x00000017, 0x00000013, 0x00000015, 0x0000000d, 0x0004003d, 0x00000006,
    0x00000018, 0x00000017, 0x00050088, 0x00000006, 0x00000019, 0x00000010, 0x00000018, 0x00050085,
    0x00000006, 0x0000001b, 0x00000019, 0x0000001a, 0x00050083, 0x00000006, 0x0000001d, 0x0000001b,
    0x0000001c, 0x00050041, 0x0000000e, 0x0000001f, 0x0000000b, 0x0000001e, 0x0004003d, 0x00000006,
    0x00000020, 0x0000001f, 0x00060041, 0x00000016, 0x00000021, 0x00000013, 0x00000015, 0x0000001e,
    0x0004003d, 0x00000006, 0x00000022, 0x00000021, 0x00050088, 0x00000006, 0x00000023, 0x00000020,
    0x00000022, 0x00050085, 0x00000006, 0x00000024, 0x00000023, 0x0000001a, 0x00050083, 0x00000006,
    0x00000025, 0x00000024, 0x0000001c, 0x00050050, 0x00000007, 0x00000026, 0x0000001d, 0x00000025,
    0x0003003e, 0x00000009, 0x00000026, 0x00050041, 0x0000002c, 0x0000002d, 0x00000009, 0x0000000d,
    0x0004003d, 0x00000006, 0x0000002e, 0x0000002d, 0x00050041, 0x0000002c, 0x0000002f, 0x00000009,
    0x0000001e, 0x0004003d, 0x00000006, 0x00000030, 0x0000002f, 0x00070050, 0x00000027, 0x00000032,
    0x0000002e, 0x00000030, 0x00000031, 0x0000001c, 0x00050041, 0x00000033, 0x00000034, 0x0000002b,
    0x00000015, 0x0003003e, 0x00000034, 0x00000032, 0x0004003d, 0x00000007, 0x00000038, 0x00000037,
    0x0003003e, 0x00000036, 0x00000038, 0x0004003d, 0x00000006, 0x0000003c, 0x0000003b, 0x0003003e,
    0x0000003a, 0x0000003c, 0x0004003d, 0x00000014, 0x00000041, 0x00000040, 0x0004007c, 0x0000000c,
    0x00000042, 0x00000041, 0x0003003e, 0x0000003e, 0x00000042, 0x000100fd, 0x00010038,
];

const NATIVE_VULKAN_VULKANALIA_PLANE_SCENE_VIDEO_LAYER_FRAGMENT_SPIRV: [u32; 496] = [
    0x07230203, 0x00010000, 0x000d000b, 0x00000050, 0x00000000, 0x00020011, 0x00000001, 0x0006000b,
    0x00000001, 0x4c534c47, 0x6474732e, 0x3035342e, 0x00000000, 0x0003000e, 0x00000000, 0x00000001,
    0x0009000f, 0x00000004, 0x00000004, 0x6e69616d, 0x00000000, 0x0000000c, 0x00000010, 0x00000048,
    0x0000004d, 0x00030010, 0x00000004, 0x00000007, 0x00030003, 0x00000002, 0x000001c2, 0x000a0004,
    0x475f4c47, 0x4c474f4f, 0x70635f45, 0x74735f70, 0x5f656c79, 0x656e696c, 0x7269645f, 0x69746365,
    0x00006576, 0x00080004, 0x475f4c47, 0x4c474f4f, 0x6e695f45, 0x64756c63, 0x69645f65, 0x74636572,
    0x00657669, 0x00040005, 0x00000004, 0x6e69616d, 0x00000000, 0x00040005, 0x00000009, 0x726f6f63,
    0x00000064, 0x00040005, 0x0000000c, 0x755f6e69, 0x00000076, 0x00050005, 0x00000010, 0x6c5f6e69,
    0x72657961, 0x00000000, 0x00030005, 0x00000017, 0x00000079, 0x00040005, 0x0000001b, 0x6c705f79,
    0x00656e61, 0x00030005, 0x00000023, 0x00007675, 0x00050005, 0x00000024, 0x705f7675, 0x656e616c,
    0x00000000, 0x00030005, 0x0000002c, 0x00000072, 0x00030005, 0x00000034, 0x00000067, 0x00030005,
    0x00000040, 0x00000062, 0x00050005, 0x00000048, 0x5f74756f, 0x6f6c6f63, 0x00000072, 0x00050005,
    0x0000004d, 0x6f5f6e69, 0x69636170, 0x00007974, 0x00040047, 0x0000000c, 0x0000001e, 0x00000000,
    0x00030047, 0x00000010, 0x0000000e, 0x00040047, 0x00000010, 0x0000001e, 0x00000002, 0x00040047,
    0x0000001b, 0x00000021, 0x00000000, 0x00040047, 0x0000001b, 0x00000022, 0x00000000, 0x00040047,
    0x00000024, 0x00000021, 0x00000001, 0x00040047, 0x00000024, 0x00000022, 0x00000000, 0x00040047,
    0x00000048, 0x0000001e, 0x00000000, 0x00040047, 0x0000004d, 0x0000001e, 0x00000001, 0x00020013,
    0x00000002, 0x00030021, 0x00000003, 0x00000002, 0x00030016, 0x00000006, 0x00000020, 0x00040017,
    0x00000007, 0x00000006, 0x00000003, 0x00040020, 0x00000008, 0x00000007, 0x00000007, 0x00040017,
    0x0000000a, 0x00000006, 0x00000002, 0x00040020, 0x0000000b, 0x00000001, 0x0000000a, 0x0004003b,
    0x0000000b, 0x0000000c, 0x00000001, 0x00040015, 0x0000000e, 0x00000020, 0x00000000, 0x00040020,
    0x0000000f, 0x00000001, 0x0000000e, 0x0004003b, 0x0000000f, 0x00000010, 0x00000001, 0x00040020,
    0x00000016, 0x00000007, 0x00000006, 0x00090019, 0x00000018, 0x00000006, 0x00000001, 0x00000000,
    0x00000001, 0x00000000, 0x00000001, 0x00000000, 0x0003001b, 0x00000019, 0x00000018, 0x00040020,
    0x0000001a, 0x00000000, 0x00000019, 0x0004003b, 0x0000001a, 0x0000001b, 0x00000000, 0x00040017,
    0x0000001e, 0x00000006, 0x00000004, 0x0004002b, 0x0000000e, 0x00000020, 0x00000000, 0x00040020,
    0x00000022, 0x00000007, 0x0000000a, 0x0004003b, 0x0000001a, 0x00000024, 0x00000000, 0x0004002b,
    0x00000006, 0x00000029, 0x3f000000, 0x0005002c, 0x0000000a, 0x0000002a, 0x00000029, 0x00000029,
    0x0004002b, 0x00000006, 0x0000002e, 0x3fc9930c, 0x0004002b, 0x0000000e, 0x0000002f, 0x00000001,
    0x0004002b, 0x00000006, 0x00000036, 0x3e3fcb92, 0x0004002b, 0x00000006, 0x0000003b, 0x3eefaace,
    0x0004002b, 0x00000006, 0x00000042, 0x3fed844d, 0x00040020, 0x00000047, 0x00000003, 0x0000001e,
    0x0004003b, 0x00000047, 0x00000048, 0x00000003, 0x00040020, 0x0000004c, 0x00000001, 0x00000006,
    0x0004003b, 0x0000004c, 0x0000004d, 0x00000001, 0x00050036, 0x00000002, 0x00000004, 0x00000000,
    0x00000003, 0x000200f8, 0x00000005, 0x0004003b, 0x00000008, 0x00000009, 0x00000007, 0x0004003b,
    0x00000016, 0x00000017, 0x00000007, 0x0004003b, 0x00000022, 0x00000023, 0x00000007, 0x0004003b,
    0x00000016, 0x0000002c, 0x00000007, 0x0004003b, 0x00000016, 0x00000034, 0x00000007, 0x0004003b,
    0x00000016, 0x00000040, 0x00000007, 0x0004003d, 0x0000000a, 0x0000000d, 0x0000000c, 0x0004003d,
    0x0000000e, 0x00000011, 0x00000010, 0x00040070, 0x00000006, 0x00000012, 0x00000011, 0x00050051,
    0x00000006, 0x00000013, 0x0000000d, 0x00000000, 0x00050051, 0x00000006, 0x00000014, 0x0000000d,
    0x00000001, 0x00060050, 0x00000007, 0x00000015, 0x00000013, 0x00000014, 0x00000012, 0x0003003e,
    0x00000009, 0x00000015, 0x0004003d, 0x00000019, 0x0000001c, 0x0000001b, 0x0004003d, 0x00000007,
    0x0000001d, 0x00000009, 0x00050057, 0x0000001e, 0x0000001f, 0x0000001c, 0x0000001d, 0x00050051,
    0x00000006, 0x00000021, 0x0000001f, 0x00000000, 0x0003003e, 0x00000017, 0x00000021, 0x0004003d,
    0x00000019, 0x00000025, 0x00000024, 0x0004003d, 0x00000007, 0x00000026, 0x00000009, 0x00050057,
    0x0000001e, 0x00000027, 0x00000025, 0x00000026, 0x0007004f, 0x0000000a, 0x00000028, 0x00000027,
    0x00000027, 0x00000000, 0x00000001, 0x00050083, 0x0000000a, 0x0000002b, 0x00000028, 0x0000002a,
    0x0003003e, 0x00000023, 0x0000002b, 0x0004003d, 0x00000006, 0x0000002d, 0x00000017, 0x00050041,
    0x00000016, 0x00000030, 0x00000023, 0x0000002f, 0x0004003d, 0x00000006, 0x00000031, 0x00000030,
    0x00050085, 0x00000006, 0x00000032, 0x0000002e, 0x00000031, 0x00050081, 0x00000006, 0x00000033,
    0x0000002d, 0x00000032, 0x0003003e, 0x0000002c, 0x00000033, 0x0004003d, 0x00000006, 0x00000035,
    0x00000017, 0x00050041, 0x00000016, 0x00000037, 0x00000023, 0x00000020, 0x0004003d, 0x00000006,
    0x00000038, 0x00000037, 0x00050085, 0x00000006, 0x00000039, 0x00000036, 0x00000038, 0x00050083,
    0x00000006, 0x0000003a, 0x00000035, 0x00000039, 0x00050041, 0x00000016, 0x0000003c, 0x00000023,
    0x0000002f, 0x0004003d, 0x00000006, 0x0000003d, 0x0000003c, 0x00050085, 0x00000006, 0x0000003e,
    0x0000003b, 0x0000003d, 0x00050083, 0x00000006, 0x0000003f, 0x0000003a, 0x0000003e, 0x0003003e,
    0x00000034, 0x0000003f, 0x0004003d, 0x00000006, 0x00000041, 0x00000017, 0x00050041, 0x00000016,
    0x00000043, 0x00000023, 0x00000020, 0x0004003d, 0x00000006, 0x00000044, 0x00000043, 0x00050085,
    0x00000006, 0x00000045, 0x00000042, 0x00000044, 0x00050081, 0x00000006, 0x00000046, 0x00000041,
    0x00000045, 0x0003003e, 0x00000040, 0x00000046, 0x0004003d, 0x00000006, 0x00000049, 0x0000002c,
    0x0004003d, 0x00000006, 0x0000004a, 0x00000034, 0x0004003d, 0x00000006, 0x0000004b, 0x00000040,
    0x0004003d, 0x00000006, 0x0000004e, 0x0000004d, 0x00070050, 0x0000001e, 0x0000004f, 0x00000049,
    0x0000004a, 0x0000004b, 0x0000004e, 0x0003003e, 0x00000048, 0x0000004f, 0x000100fd, 0x00010038,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoded_image_present_order_keeps_queue_ownership_explicit() {
        let split = native_vulkan_vulkanalia_decoded_image_present_command_order(
            false, "disabled", "disabled", false, false,
        );
        assert!(split.contains(&"cmd_pipeline_barrier2_video_release"));
        assert!(split.contains(&"cmd_pipeline_barrier2_graphics_acquire_shader_read"));
        assert!(split.contains(&"cmd_begin_rendering"));
        assert!(split.contains(&"cmd_bind_resource_heap_ext"));
        assert!(split.contains(&"cmd_bind_sampler_heap_ext"));
        assert!(split.contains(&"draw_with_descriptor_heap_plane_array_sampler_mapping"));
        assert!(split.contains(&"cmd_pipeline_barrier2_decoded_restore"));
        assert!(split.contains(&"queue_submit2_present"));
        assert!(split.contains(&"defer_frame_slot_reuse_until_render_fence"));
        assert!(split.contains(&"no_queue_wait_idle_after_present"));

        let same = native_vulkan_vulkanalia_decoded_image_present_command_order(
            true, "disabled", "disabled", false, false,
        );
        assert!(!same.contains(&"cmd_pipeline_barrier2_video_release"));
        assert!(same.contains(&"cmd_bind_resource_heap_ext"));
        assert!(same.contains(&"cmd_bind_sampler_heap_ext"));
        assert!(same.contains(&"draw_with_descriptor_heap_plane_array_sampler_mapping"));
        assert!(same.contains(&"defer_frame_slot_reuse_until_render_fence"));

        let video_layer = native_vulkan_vulkanalia_decoded_image_present_command_order(
            true, "disabled", "disabled", true, false,
        );
        assert!(video_layer.contains(&"cmd_bind_scene_video_layer_pipeline"));
        assert!(video_layer.contains(&"cmd_draw_scene_video_layers_inside_video_rendering"));
        assert!(!video_layer.contains(&"draw_with_descriptor_heap_plane_array_sampler_mapping"));

        let present_id2 = native_vulkan_vulkanalia_decoded_image_present_command_order(
            true,
            "present-id2-khr",
            "present-wait2-khr",
            false,
            false,
        );
        assert!(present_id2.contains(&"present_id2_khr"));
        assert!(present_id2.contains(&"wait_for_present2_khr"));
        assert!(present_id2.windows(3).any(|triple| triple
            == [
                "present_id2_khr",
                "queue_present_khr",
                "wait_for_present2_khr"
            ]));
    }

    #[test]
    fn shader_module_code_size_uses_bytes_not_words() {
        assert_eq!(
            native_vulkan_vulkanalia_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_PLANE_PRESENT_VERTEX_SPIRV
            ),
            NATIVE_VULKAN_VULKANALIA_PLANE_PRESENT_VERTEX_SPIRV.len() * 4
        );
        assert_eq!(
            NATIVE_VULKAN_VULKANALIA_PLANE_SCENE_VIDEO_LAYER_VERTEX_SPIRV[0],
            0x07230203
        );
        assert_eq!(
            NATIVE_VULKAN_VULKANALIA_PLANE_SCENE_VIDEO_LAYER_FRAGMENT_SPIRV[0],
            0x07230203
        );
        assert_eq!(
            native_vulkan_vulkanalia_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_PLANE_SCENE_VIDEO_LAYER_VERTEX_SPIRV
            ),
            NATIVE_VULKAN_VULKANALIA_PLANE_SCENE_VIDEO_LAYER_VERTEX_SPIRV.len() * 4
        );
        assert_eq!(
            native_vulkan_vulkanalia_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_PLANE_SCENE_VIDEO_LAYER_FRAGMENT_SPIRV
            ),
            NATIVE_VULKAN_VULKANALIA_PLANE_SCENE_VIDEO_LAYER_FRAGMENT_SPIRV.len() * 4
        );
    }
}
