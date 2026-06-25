#![allow(dead_code)]

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder, KhrSwapchainExtensionDeviceCommands};

use super::video_decode_submit::FFMPEG_VULKAN_DECODE_REFERENCE;
use super::video_present_handoff::NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot;
use super::video_session_images::VulkanaliaVideoSessionResourceImage;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaDecodedImagePresentSamplerSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub source_image_role: &'static str,
    pub picture_format: String,
    pub sampled_array_layer: u32,
    pub conversion_created: bool,
    pub sampler_created: bool,
    pub sampled_view_created: bool,
    pub descriptor_set_layout_created: bool,
    pub descriptor_pool_created: bool,
    pub descriptor_set_allocated: bool,
    pub descriptor_pool_combined_image_sampler_budget: u32,
    pub ycbcr_model: &'static str,
    pub ycbcr_range: &'static str,
    pub x_chroma_offset: &'static str,
    pub y_chroma_offset: &'static str,
    pub chroma_filter: &'static str,
    pub descriptor_type: &'static str,
    pub image_layout_for_shader: &'static str,
    pub present_pass_model: &'static str,
    pub queue_transfer_model: &'static str,
    pub uses_dynamic_rendering: bool,
    pub uses_synchronization2: bool,
    pub uses_submit2: bool,
    pub ffmpeg_reference: &'static str,
}

pub(super) struct VulkanaliaDecodedImagePresentSamplerResources {
    pub(super) conversion: vk::SamplerYcbcrConversion,
    pub(super) sampler: vk::Sampler,
    pub(super) sampled_view: vk::ImageView,
    pub(super) descriptor_set_layout: vk::DescriptorSetLayout,
    pub(super) descriptor_pool: vk::DescriptorPool,
    pub(super) descriptor_set: vk::DescriptorSet,
    pub(super) snapshot: NativeVulkanVulkanaliaDecodedImagePresentSamplerSnapshot,
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
    pub uses_pipeline_rendering_create_info: bool,
    pub uses_dynamic_rendering: bool,
    pub uses_ycbcr_sampler_descriptor: bool,
    pub ffmpeg_reference: &'static str,
}

pub(super) struct VulkanaliaDecodedImagePresentPipelineResources {
    pub(super) pipeline_layout: vk::PipelineLayout,
    pub(super) pipeline: vk::Pipeline,
    pub(super) snapshot: NativeVulkanVulkanaliaDecodedImagePresentPipelineSnapshot,
}

pub(super) struct VulkanaliaDecodedImagePresentFrameResources {
    swapchain_image_views: Vec<vk::ImageView>,
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    image_available: Vec<vk::Semaphore>,
    render_finished: Vec<vk::Semaphore>,
    in_flight: Vec<vk::Fence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct VulkanaliaDecodedImagePresentTimingConfig {
    pub(super) present_id_enabled: bool,
    pub(super) present_id2_enabled: bool,
    pub(super) present_wait_enabled: bool,
    pub(super) present_wait2_enabled: bool,
}

impl VulkanaliaDecodedImagePresentTimingConfig {
    pub(super) fn disabled() -> Self {
        Self {
            present_id_enabled: false,
            present_id2_enabled: false,
            present_wait_enabled: false,
            present_wait2_enabled: false,
        }
    }

    pub(super) fn new(
        present_id_enabled: bool,
        present_id2_enabled: bool,
        present_wait_enabled: bool,
        present_wait2_enabled: bool,
    ) -> Self {
        Self {
            present_id_enabled,
            present_id2_enabled,
            present_wait_enabled,
            present_wait2_enabled,
        }
    }

    fn present_id(self, present_frame_index: u32) -> Option<u64> {
        if self.present_id2_enabled || self.present_id_enabled {
            Some(u64::from(present_frame_index).saturating_add(1))
        } else {
            None
        }
    }

    fn present_id_mode(self) -> &'static str {
        if self.present_id2_enabled {
            "present-id2-khr"
        } else if self.present_id_enabled {
            "present-id-khr"
        } else {
            "disabled"
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub present_frame_index: u32,
    pub sampled_array_layer: u32,
    pub sampled_array_layer_source: &'static str,
    pub source_frame_pts_ms: Option<u64>,
    pub source_frame_duration_ms: Option<u64>,
    pub display_order_key: i64,
    pub display_order_key_source: &'static str,
    pub pacing_sleep_micros: u64,
    pub pacing_clock_model: &'static str,
    pub present_frame_slot: u32,
    pub present_sync_model: &'static str,
    pub wait_idle_after_present: bool,
    pub present_id: Option<u64>,
    pub present_id_mode: &'static str,
    pub uses_present_id: bool,
    pub uses_present_id2: bool,
    pub present_wait_available: bool,
    pub present_wait2_available: bool,
    pub present_wait_after_present: bool,
    pub swapchain_image_index: u32,
    pub swapchain_image_view_count: usize,
    pub target_format: String,
    pub extent: (u32, u32),
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
    pub ffmpeg_reference: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaDecodedImagePresentSequenceSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub requested_present_frame_count: u32,
    pub submitted_present_frame_count: u32,
    pub presented_frame_count: u32,
    pub sampled_array_layers: Vec<u32>,
    pub source_frame_pts_ms: Vec<Option<u64>>,
    pub source_frame_duration_ms: Vec<Option<u64>>,
    pub display_order_keys: Vec<i64>,
    pub display_order_key_sources: Vec<&'static str>,
    pub present_ids: Vec<Option<u64>>,
    pub total_pacing_sleep_micros: u64,
    pub pts_monotonic: bool,
    pub display_order_monotonic: bool,
    pub uses_present_id: bool,
    pub uses_present_id2: bool,
    pub present_wait_available: bool,
    pub present_wait2_available: bool,
    pub present_wait_after_present: bool,
    pub present_handoff: NativeVulkanVulkanaliaDecodedPresentHandoffSnapshot,
    pub draws: Vec<NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot>,
    pub frame_order_model: &'static str,
    pub present_resource_reuse_model: &'static str,
    pub all_zero_copy_presented: bool,
    pub uses_dynamic_rendering: bool,
    pub uses_synchronization2: bool,
    pub uses_submit2: bool,
    pub ffmpeg_reference: &'static str,
}

pub(super) fn native_vulkan_vulkanalia_create_decoded_image_present_pipeline_resources(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
    descriptor_set_layout: vk::DescriptorSetLayout,
) -> Result<VulkanaliaDecodedImagePresentPipelineResources, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("decoded image present pipeline requires non-zero extent".to_owned());
    }

    let set_layouts = [descriptor_set_layout];
    let pipeline_layout_info = vk::PipelineLayoutCreateInfo::builder().set_layouts(&set_layouts);
    let pipeline_layout = unsafe { device.create_pipeline_layout(&pipeline_layout_info, None) }
        .map_err(|err| {
            format!("vkCreatePipelineLayout(vulkanalia decoded present dynamic rendering): {err:?}")
        })?;

    let result = (|| -> Result<VulkanaliaDecodedImagePresentPipelineResources, String> {
        let vertex_module = native_vulkan_vulkanalia_create_shader_module(
            device,
            &NATIVE_VULKAN_VULKANALIA_YCBCR_PRESENT_VERTEX_SPIRV,
            "decoded present vertex",
        )?;
        let result = (|| -> Result<VulkanaliaDecodedImagePresentPipelineResources, String> {
            let fragment_module = native_vulkan_vulkanalia_create_shader_module(
                device,
                &NATIVE_VULKAN_VULKANALIA_YCBCR_PRESENT_FRAGMENT_SPIRV,
                "decoded present fragment",
            )?;
            let result = (|| -> Result<VulkanaliaDecodedImagePresentPipelineResources, String> {
                let shader_entry = b"main\0";
                let stages = [
                    vk::PipelineShaderStageCreateInfo::builder()
                        .stage(vk::ShaderStageFlags::VERTEX)
                        .module(vertex_module)
                        .name(shader_entry)
                        .build(),
                    vk::PipelineShaderStageCreateInfo::builder()
                        .stage(vk::ShaderStageFlags::FRAGMENT)
                        .module(fragment_module)
                        .name(shader_entry)
                        .build(),
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
                let pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
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
                    .push_next(&mut rendering_info)
                    .build();
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
                Ok(VulkanaliaDecodedImagePresentPipelineResources {
                    pipeline_layout,
                    pipeline,
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
                        fragment_shader_model: "single combined YCbCr sampler2D",
                        descriptor_sets: 1,
                        uses_pipeline_rendering_create_info: true,
                        uses_dynamic_rendering: true,
                        uses_ycbcr_sampler_descriptor: true,
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

pub(super) fn native_vulkan_vulkanalia_destroy_decoded_image_present_pipeline_resources(
    device: &Device,
    resources: VulkanaliaDecodedImagePresentPipelineResources,
) {
    unsafe {
        device.destroy_pipeline(resources.pipeline, None);
        device.destroy_pipeline_layout(resources.pipeline_layout, None);
    }
}

pub(super) fn native_vulkan_vulkanalia_create_decoded_image_present_frame_resources(
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

        Ok(VulkanaliaDecodedImagePresentFrameResources {
            swapchain_image_views: std::mem::take(&mut swapchain_image_views),
            command_pool,
            command_buffers,
            image_available: std::mem::take(&mut image_available),
            render_finished: std::mem::take(&mut render_finished),
            in_flight: std::mem::take(&mut in_flight),
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
        );
    }

    result
}

#[allow(clippy::too_many_arguments)]
pub(super) fn native_vulkan_vulkanalia_present_decoded_image_once(
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
        None,
        None,
        0,
        "single-frame-present",
        0,
        "unpaced-single-frame-smoke",
        present_timing,
    );
    native_vulkan_vulkanalia_destroy_decoded_image_present_frame_resources(device, frame_resources);
    result
}

#[allow(clippy::too_many_arguments)]
pub(super) fn native_vulkan_vulkanalia_present_decoded_image_frame(
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
    source_frame_pts_ms: Option<u64>,
    source_frame_duration_ms: Option<u64>,
    display_order_key: i64,
    display_order_key_source: &'static str,
    pacing_sleep_micros: u64,
    pacing_clock_model: &'static str,
    present_timing: VulkanaliaDecodedImagePresentTimingConfig,
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
    let render_finished = frame_resources.render_finished[present_frame_slot];
    let in_flight = frame_resources.in_flight[present_frame_slot];

    unsafe {
        device
            .wait_for_fences(&[in_flight], true, u64::MAX)
            .map_err(|err| format!("vkWaitForFences(vulkanalia decoded image present): {err:?}"))?;
        device
            .reset_fences(&[in_flight])
            .map_err(|err| format!("vkResetFences(vulkanalia decoded image present): {err:?}"))?;
    }
    let (image_index, _) = unsafe {
        device.acquire_next_image_khr(swapchain, u64::MAX, image_available, vk::Fence::null())
    }
    .map_err(|err| format!("vkAcquireNextImageKHR(vulkanalia decoded image present): {err:?}"))?;
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

    native_vulkan_vulkanalia_record_decoded_image_present_command_buffer(
        device,
        command_buffer,
        swapchain_image,
        swapchain_view,
        swapchain_extent,
        resource_image.image,
        sampled_array_layer,
        sampler.descriptor_set,
        pipeline.pipeline_layout,
        pipeline.pipeline,
    )?;
    native_vulkan_vulkanalia_submit_decoded_image_present_command_buffer2(
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
    let present_id = present_timing.present_id(present_frame_index);
    let present_ids = [present_id.unwrap_or(0)];
    let mut present_id2_info = present_id.map(|_| {
        vk::PresentId2KHR::builder()
            .present_ids(&present_ids)
            .build()
    });
    let mut present_id_info = present_id.map(|_| {
        vk::PresentIdKHR::builder()
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
    } else if present_timing.present_id_enabled {
        if let Some(present_id_info) = present_id_info.as_mut() {
            present_info = present_info.push_next(present_id_info);
        }
    }
    unsafe {
        device
            .queue_present_khr(queue, &present_info)
            .map_err(|err| {
                format!("vkQueuePresentKHR(vulkanalia decoded image present): {err:?}")
            })?;
    }

    Ok(NativeVulkanVulkanaliaDecodedImagePresentDrawSnapshot {
        binding: "vulkanalia",
        route: "decoded-image-dynamic-rendering-present-draw",
        present_frame_index,
        sampled_array_layer,
        sampled_array_layer_source: "submitted-dst-base-array-layer",
        source_frame_pts_ms,
        source_frame_duration_ms,
        display_order_key,
        display_order_key_source,
        pacing_sleep_micros,
        pacing_clock_model,
        present_frame_slot: present_frame_slot as u32,
        present_sync_model: "frame-slot semaphore/fence reuse; no per-present queue_wait_idle",
        wait_idle_after_present: false,
        present_id,
        present_id_mode: present_timing.present_id_mode(),
        uses_present_id: present_timing.present_id_enabled,
        uses_present_id2: present_timing.present_id2_enabled,
        present_wait_available: present_timing.present_wait_enabled,
        present_wait2_available: present_timing.present_wait2_enabled,
        present_wait_after_present: false,
        swapchain_image_index: image_index,
        swapchain_image_view_count: frame_resources.swapchain_image_views.len(),
        target_format: format!("{swapchain_format:?}"),
        extent: (swapchain_extent.width, swapchain_extent.height),
        command_buffer_recorded: true,
        submitted: true,
        presented: true,
        decoded_image_layout_transition: "video-decode-dpb -> shader-read-only-optimal -> video-decode-dpb",
        swapchain_layout_transition: "undefined -> color-attachment-optimal -> present-src-khr",
        render_model: "immutable YCbCr combined image sampler -> Vulkan 1.3/1.4 dynamic rendering fullscreen triangle -> Wayland swapchain",
        command_order: native_vulkan_vulkanalia_decoded_image_present_command_order(
            true,
            present_timing.present_id_mode(),
        ),
        uses_pipeline_rendering_create_info: true,
        uses_dynamic_rendering: true,
        uses_synchronization2: true,
        uses_submit2: true,
        zero_copy_presented: true,
        ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
    })
}

pub(super) fn native_vulkan_vulkanalia_destroy_decoded_image_present_frame_resources(
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
    );
}

fn native_vulkan_vulkanalia_destroy_partial_decoded_image_present_frame_resources(
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

pub(super) fn native_vulkan_vulkanalia_create_decoded_image_present_sampler_resources(
    device: &Device,
    resource_image: &VulkanaliaVideoSessionResourceImage,
    picture_format: vk::Format,
    sampled_array_layer: u32,
    video_queue_family_index: u32,
    present_queue_family_index: u32,
) -> Result<VulkanaliaDecodedImagePresentSamplerResources, String> {
    if !resource_image
        .snapshot
        .image_usage_flags
        .contains(&"sampled")
    {
        return Err("decoded image present sampler requires SAMPLED image usage".to_owned());
    }
    if sampled_array_layer >= resource_image.snapshot.array_layers {
        return Err(format!(
            "decoded image present sampler layer {sampled_array_layer} is outside {} image layers",
            resource_image.snapshot.array_layers
        ));
    }
    let ycbcr_model = native_vulkan_vulkanalia_decoded_image_ycbcr_model(picture_format)?;
    let ycbcr_range = vk::SamplerYcbcrRange::ITU_NARROW;
    let x_chroma_offset = vk::ChromaLocation::COSITED_EVEN;
    let y_chroma_offset = vk::ChromaLocation::MIDPOINT;
    let chroma_filter = vk::Filter::LINEAR;

    let conversion_info = vk::SamplerYcbcrConversionCreateInfo::builder()
        .format(picture_format)
        .ycbcr_model(ycbcr_model)
        .ycbcr_range(ycbcr_range)
        .components(vk::ComponentMapping::default())
        .x_chroma_offset(x_chroma_offset)
        .y_chroma_offset(y_chroma_offset)
        .chroma_filter(chroma_filter)
        .force_explicit_reconstruction(false);
    let conversion = unsafe { device.create_sampler_ycbcr_conversion(&conversion_info, None) }
        .map_err(|err| {
            format!("vkCreateSamplerYcbcrConversion(vulkanalia decoded present): {err:?}")
        })?;

    let result = (|| -> Result<VulkanaliaDecodedImagePresentSamplerResources, String> {
        let mut sampler_conversion_info = vk::SamplerYcbcrConversionInfo::builder()
            .conversion(conversion)
            .build();
        let sampler_info = vk::SamplerCreateInfo::builder()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .min_lod(0.0)
            .max_lod(0.0)
            .push_next(&mut sampler_conversion_info);
        let sampler = unsafe { device.create_sampler(&sampler_info, None) }
            .map_err(|err| format!("vkCreateSampler(vulkanalia decoded present ycbcr): {err:?}"))?;

        let result = (|| -> Result<VulkanaliaDecodedImagePresentSamplerResources, String> {
            let mut view_usage_info = vk::ImageViewUsageCreateInfo::builder()
                .usage(vk::ImageUsageFlags::SAMPLED)
                .build();
            let mut view_conversion_info = vk::SamplerYcbcrConversionInfo::builder()
                .conversion(conversion)
                .build();
            let sampled_view = native_vulkan_vulkanalia_create_decoded_image_sampled_view(
                device,
                resource_image.image,
                picture_format,
                sampled_array_layer,
                &mut view_usage_info,
                &mut view_conversion_info,
            )?;

            let result = (|| -> Result<VulkanaliaDecodedImagePresentSamplerResources, String> {
                let immutable_samplers = [sampler];
                let descriptor_binding = vk::DescriptorSetLayoutBinding::builder()
                    .binding(0)
                    .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                    .descriptor_count(1)
                    .stage_flags(vk::ShaderStageFlags::FRAGMENT)
                    .immutable_samplers(&immutable_samplers)
                    .build();
                let descriptor_bindings = [descriptor_binding];
                let descriptor_set_layout_info =
                    vk::DescriptorSetLayoutCreateInfo::builder().bindings(&descriptor_bindings);
                let descriptor_set_layout = unsafe {
                    device.create_descriptor_set_layout(&descriptor_set_layout_info, None)
                }
                .map_err(|err| {
                    format!(
                        "vkCreateDescriptorSetLayout(vulkanalia decoded present ycbcr): {err:?}"
                    )
                })?;

                let result =
                    (|| -> Result<VulkanaliaDecodedImagePresentSamplerResources, String> {
                        let descriptor_budget =
                            NATIVE_VULKAN_VULKANALIA_YCBCR_DESCRIPTOR_POOL_BUDGET;
                        let pool_size = vk::DescriptorPoolSize::builder()
                            .type_(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                            .descriptor_count(descriptor_budget)
                            .build();
                        let pool_sizes = [pool_size];
                        let descriptor_pool_info = vk::DescriptorPoolCreateInfo::builder()
                            .max_sets(1)
                            .pool_sizes(&pool_sizes);
                        let descriptor_pool = unsafe {
                            device.create_descriptor_pool(&descriptor_pool_info, None)
                        }
                        .map_err(|err| {
                            format!(
                                "vkCreateDescriptorPool(vulkanalia decoded present ycbcr): {err:?}"
                            )
                        })?;

                        let result =
                        (|| -> Result<VulkanaliaDecodedImagePresentSamplerResources, String> {
                            let descriptor_set_layouts = [descriptor_set_layout];
                            let allocate_info = vk::DescriptorSetAllocateInfo::builder()
                                .descriptor_pool(descriptor_pool)
                                .set_layouts(&descriptor_set_layouts);
                            let descriptor_sets =
                                unsafe { device.allocate_descriptor_sets(&allocate_info) }
                                    .map_err(|err| {
                                        format!(
                                            "vkAllocateDescriptorSets(vulkanalia decoded present ycbcr): {err:?}"
                                        )
                                    })?;
                            let descriptor_set = descriptor_sets[0];
                            let descriptor_image = vk::DescriptorImageInfo::builder()
                                .sampler(sampler)
                                .image_view(sampled_view)
                                .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                                .build();
                            let descriptor_images = [descriptor_image];
                            let write = vk::WriteDescriptorSet::builder()
                                .dst_set(descriptor_set)
                                .dst_binding(0)
                                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                                .image_info(&descriptor_images)
                                .build();
                            let descriptor_copies: [vk::CopyDescriptorSet; 0] = [];
                            unsafe {
                                device.update_descriptor_sets(&[write], &descriptor_copies);
                            }

                            Ok(VulkanaliaDecodedImagePresentSamplerResources {
                                conversion,
                                sampler,
                                sampled_view,
                                descriptor_set_layout,
                                descriptor_pool,
                                descriptor_set,
                                snapshot: NativeVulkanVulkanaliaDecodedImagePresentSamplerSnapshot {
                                    binding: "vulkanalia",
                                    route: "decoded-image-ycbcr-sampler-present-resource",
                                    source_image_role: resource_image.snapshot.role,
                                    picture_format: format!("{picture_format:?}"),
                                    sampled_array_layer,
                                    conversion_created: true,
                                    sampler_created: true,
                                    sampled_view_created: true,
                                    descriptor_set_layout_created: true,
                                    descriptor_pool_created: true,
                                    descriptor_set_allocated: true,
                                    descriptor_pool_combined_image_sampler_budget:
                                        descriptor_budget,
                                    ycbcr_model: sampler_ycbcr_model_label(ycbcr_model),
                                    ycbcr_range: sampler_ycbcr_range_label(ycbcr_range),
                                    x_chroma_offset: chroma_location_label(x_chroma_offset),
                                    y_chroma_offset: chroma_location_label(y_chroma_offset),
                                    chroma_filter: filter_label(chroma_filter),
                                    descriptor_type: "combined-image-sampler",
                                    image_layout_for_shader: "shader-read-only-optimal",
                                    present_pass_model:
                                        "decoded image -> immutable YCbCr combined image sampler -> dynamic rendering fullscreen graphics pass -> swapchain",
                                    queue_transfer_model:
                                        native_vulkan_vulkanalia_decoded_image_present_queue_model(
                                            video_queue_family_index,
                                            present_queue_family_index,
                                        ),
                                    uses_dynamic_rendering: true,
                                    uses_synchronization2: true,
                                    uses_submit2: true,
                                    ffmpeg_reference: FFMPEG_VULKAN_DECODE_REFERENCE,
                                },
                            })
                        })();

                        if result.is_err() {
                            unsafe {
                                device.destroy_descriptor_pool(descriptor_pool, None);
                            }
                        }
                        result
                    })();

                if result.is_err() {
                    unsafe {
                        device.destroy_descriptor_set_layout(descriptor_set_layout, None);
                    }
                }
                result
            })();

            if result.is_err() {
                unsafe {
                    device.destroy_image_view(sampled_view, None);
                }
            }
            result
        })();

        if result.is_err() {
            unsafe {
                device.destroy_sampler(sampler, None);
            }
        }
        result
    })();

    if result.is_err() {
        unsafe {
            device.destroy_sampler_ycbcr_conversion(conversion, None);
        }
    }
    result
}

pub(super) fn native_vulkan_vulkanalia_retarget_decoded_image_present_sampler_layer(
    device: &Device,
    resource_image: &VulkanaliaVideoSessionResourceImage,
    picture_format: vk::Format,
    resources: &mut VulkanaliaDecodedImagePresentSamplerResources,
    sampled_array_layer: u32,
) -> Result<(), String> {
    if sampled_array_layer >= resource_image.snapshot.array_layers {
        return Err(format!(
            "decoded image present sampler layer {sampled_array_layer} is outside {} image layers",
            resource_image.snapshot.array_layers
        ));
    }
    if sampled_array_layer == resources.snapshot.sampled_array_layer {
        return Ok(());
    }

    let mut view_usage_info = vk::ImageViewUsageCreateInfo::builder()
        .usage(vk::ImageUsageFlags::SAMPLED)
        .build();
    let mut view_conversion_info = vk::SamplerYcbcrConversionInfo::builder()
        .conversion(resources.conversion)
        .build();
    let sampled_view = native_vulkan_vulkanalia_create_decoded_image_sampled_view(
        device,
        resource_image.image,
        picture_format,
        sampled_array_layer,
        &mut view_usage_info,
        &mut view_conversion_info,
    )?;

    let descriptor_image = vk::DescriptorImageInfo::builder()
        .sampler(resources.sampler)
        .image_view(sampled_view)
        .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
        .build();
    let descriptor_images = [descriptor_image];
    let write = vk::WriteDescriptorSet::builder()
        .dst_set(resources.descriptor_set)
        .dst_binding(0)
        .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
        .image_info(&descriptor_images)
        .build();
    let descriptor_copies: [vk::CopyDescriptorSet; 0] = [];
    unsafe {
        device.update_descriptor_sets(&[write], &descriptor_copies);
        device.destroy_image_view(resources.sampled_view, None);
    }

    resources.sampled_view = sampled_view;
    resources.snapshot.sampled_array_layer = sampled_array_layer;
    Ok(())
}

fn native_vulkan_vulkanalia_create_decoded_image_sampled_view(
    device: &Device,
    image: vk::Image,
    picture_format: vk::Format,
    sampled_array_layer: u32,
    view_usage_info: &mut vk::ImageViewUsageCreateInfo,
    view_conversion_info: &mut vk::SamplerYcbcrConversionInfo,
) -> Result<vk::ImageView, String> {
    let subresource_range = vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(sampled_array_layer)
        .layer_count(1)
        .build();
    let sampled_view_info = vk::ImageViewCreateInfo::builder()
        .image(image)
        .view_type(vk::ImageViewType::_2D)
        .format(picture_format)
        .subresource_range(subresource_range)
        .push_next(view_usage_info)
        .push_next(view_conversion_info);
    unsafe { device.create_image_view(&sampled_view_info, None) }
        .map_err(|err| format!("vkCreateImageView(vulkanalia decoded present ycbcr): {err:?}"))
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
    descriptor_set: vk::DescriptorSet,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
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
            .src_stage_mask(vk::PipelineStageFlags2::VIDEO_DECODE_KHR)
            .src_access_mask(
                vk::AccessFlags2::VIDEO_DECODE_READ_KHR | vk::AccessFlags2::VIDEO_DECODE_WRITE_KHR,
            )
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
                float32: [0.0, 0.0, 0.0, 1.0],
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
        device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, pipeline);
        device.cmd_bind_descriptor_sets(
            command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            pipeline_layout,
            0,
            &[descriptor_set],
            &[],
        );
        device.cmd_draw(command_buffer, 3, 1, 0, 0);
        device.cmd_end_rendering(command_buffer);

        let decoded_to_decode = vk::ImageMemoryBarrier2::builder()
            .src_stage_mask(vk::PipelineStageFlags2::FRAGMENT_SHADER)
            .src_access_mask(vk::AccessFlags2::SHADER_SAMPLED_READ)
            .dst_stage_mask(vk::PipelineStageFlags2::VIDEO_DECODE_KHR)
            .dst_access_mask(
                vk::AccessFlags2::VIDEO_DECODE_READ_KHR | vk::AccessFlags2::VIDEO_DECODE_WRITE_KHR,
            )
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

fn native_vulkan_vulkanalia_submit_decoded_image_present_command_buffer2(
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
            .map_err(|err| format!("vkQueueSubmit2(vulkanalia decoded image present): {err:?}"))?;
    }

    Ok(())
}

pub(super) fn native_vulkan_vulkanalia_destroy_decoded_image_present_sampler_resources(
    device: &Device,
    resources: VulkanaliaDecodedImagePresentSamplerResources,
) {
    unsafe {
        device.destroy_descriptor_pool(resources.descriptor_pool, None);
        device.destroy_descriptor_set_layout(resources.descriptor_set_layout, None);
        device.destroy_image_view(resources.sampled_view, None);
        device.destroy_sampler(resources.sampler, None);
        device.destroy_sampler_ycbcr_conversion(resources.conversion, None);
    }
}

pub(super) fn native_vulkan_vulkanalia_decoded_image_present_command_order(
    same_queue_family: bool,
    present_id_mode: &'static str,
) -> Vec<&'static str> {
    let mut order = if same_queue_family {
        vec![
            "queue_submit2_decode",
            "cmd_pipeline_barrier2_shader_read",
            "cmd_begin_rendering",
            "cmd_bind_ycbcr_descriptor",
            "cmd_draw_fullscreen_triangle",
            "cmd_end_rendering",
            "cmd_pipeline_barrier2_decoded_restore",
            "cmd_pipeline_barrier2_present",
            "queue_submit2_present",
            "queue_present_khr",
            "defer_frame_slot_reuse_until_fence",
            "no_queue_wait_idle_after_present",
        ]
    } else {
        vec![
            "queue_submit2_decode",
            "cmd_pipeline_barrier2_video_release",
            "cmd_pipeline_barrier2_graphics_acquire_shader_read",
            "cmd_begin_rendering",
            "cmd_bind_ycbcr_descriptor",
            "cmd_draw_fullscreen_triangle",
            "cmd_end_rendering",
            "cmd_pipeline_barrier2_decoded_restore",
            "cmd_pipeline_barrier2_present",
            "queue_submit2_present",
            "queue_present_khr",
            "defer_frame_slot_reuse_until_fence",
            "no_queue_wait_idle_after_present",
        ]
    };
    match present_id_mode {
        "present-id2-khr" => order.insert(order.len().saturating_sub(3), "present_id2_khr"),
        "present-id-khr" => order.insert(order.len().saturating_sub(3), "present_id_khr"),
        _ => {}
    }
    order
}

const NATIVE_VULKAN_VULKANALIA_YCBCR_DESCRIPTOR_POOL_BUDGET: u32 = 4;

fn native_vulkan_vulkanalia_decoded_image_ycbcr_model(
    picture_format: vk::Format,
) -> Result<vk::SamplerYcbcrModelConversion, String> {
    match picture_format {
        vk::Format::G8_B8R8_2PLANE_420_UNORM
        | vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16 => {
            Ok(vk::SamplerYcbcrModelConversion::YCBCR_709)
        }
        _ => Err(format!(
            "{picture_format:?} is not a retained decoded NV12/P010 YCbCr format"
        )),
    }
}

fn native_vulkan_vulkanalia_decoded_image_present_queue_model(
    video_queue_family_index: u32,
    present_queue_family_index: u32,
) -> &'static str {
    if video_queue_family_index == present_queue_family_index {
        "single queue family: decode submission orders shader-read barrier before dynamic rendering"
    } else {
        "dedicated video queue releases decoded image, graphics/present queue acquires it with sync2 before dynamic rendering"
    }
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

fn sampler_ycbcr_model_label(model: vk::SamplerYcbcrModelConversion) -> &'static str {
    match model {
        vk::SamplerYcbcrModelConversion::YCBCR_709 => "ycbcr-709",
        vk::SamplerYcbcrModelConversion::YCBCR_601 => "ycbcr-601",
        vk::SamplerYcbcrModelConversion::YCBCR_2020 => "ycbcr-2020",
        vk::SamplerYcbcrModelConversion::YCBCR_IDENTITY => "ycbcr-identity",
        vk::SamplerYcbcrModelConversion::RGB_IDENTITY => "rgb-identity",
        _ => "unknown",
    }
}

fn sampler_ycbcr_range_label(range: vk::SamplerYcbcrRange) -> &'static str {
    match range {
        vk::SamplerYcbcrRange::ITU_FULL => "itu-full",
        vk::SamplerYcbcrRange::ITU_NARROW => "itu-narrow",
        _ => "unknown",
    }
}

fn chroma_location_label(location: vk::ChromaLocation) -> &'static str {
    match location {
        vk::ChromaLocation::COSITED_EVEN => "cosited-even",
        vk::ChromaLocation::MIDPOINT => "midpoint",
        _ => "unknown",
    }
}

fn filter_label(filter: vk::Filter) -> &'static str {
    match filter {
        vk::Filter::NEAREST => "nearest",
        vk::Filter::LINEAR => "linear",
        _ => "unknown",
    }
}

const NATIVE_VULKAN_VULKANALIA_YCBCR_PRESENT_VERTEX_SPIRV: [u32; 357] = [
    119734787, 65536, 851979, 51, 0, 131089, 1, 393227, 1, 1280527431, 1685353262, 808793134, 0,
    196622, 0, 1, 524303, 0, 4, 1852399981, 0, 32, 36, 47, 196611, 2, 450, 655364, 1197427783,
    1279741775, 1885560645, 1953718128, 1600482425, 1701734764, 1919509599, 1769235301, 25974,
    524292, 1197427783, 1279741775, 1852399429, 1685417059, 1768185701, 1952671090, 6649449,
    262149, 4, 1852399981, 0, 196613, 12, 7565168, 196613, 19, 30325, 393221, 30, 1348430951,
    1700164197, 2019914866, 0, 393222, 30, 0, 1348430951, 1953067887, 7237481, 458758, 30, 1,
    1348430951, 1953393007, 1702521171, 0, 458758, 30, 2, 1130327143, 1148217708, 1635021673,
    6644590, 458758, 30, 3, 1130327143, 1147956341, 1635021673, 6644590, 196613, 32, 0, 393221, 36,
    1449094247, 1702130277, 1684949368, 30821, 262149, 47, 1987403638, 0, 196679, 30, 2, 327752,
    30, 0, 11, 0, 327752, 30, 1, 11, 1, 327752, 30, 2, 11, 3, 327752, 30, 3, 11, 4, 262215, 36, 11,
    42, 262215, 47, 30, 0, 131091, 2, 196641, 3, 2, 196630, 6, 32, 262167, 7, 6, 2, 262165, 8, 32,
    0, 262187, 8, 9, 3, 262172, 10, 7, 9, 262176, 11, 7, 10, 262187, 6, 13, 3212836864, 327724, 7,
    14, 13, 13, 262187, 6, 15, 1077936128, 327724, 7, 16, 15, 13, 327724, 7, 17, 13, 15, 393260,
    10, 18, 14, 16, 17, 262187, 6, 20, 0, 262187, 6, 21, 1065353216, 327724, 7, 22, 20, 21, 262187,
    6, 23, 1073741824, 327724, 7, 24, 23, 21, 327724, 7, 25, 20, 13, 393260, 10, 26, 22, 24, 25,
    262167, 27, 6, 4, 262187, 8, 28, 1, 262172, 29, 6, 28, 393246, 30, 27, 6, 29, 29, 262176, 31,
    3, 30, 262203, 31, 32, 3, 262165, 33, 32, 1, 262187, 33, 34, 0, 262176, 35, 1, 33, 262203, 35,
    36, 1, 262176, 38, 7, 7, 262176, 44, 3, 27, 262176, 46, 3, 7, 262203, 46, 47, 3, 327734, 2, 4,
    0, 3, 131320, 5, 262203, 11, 12, 7, 262203, 11, 19, 7, 196670, 12, 18, 196670, 19, 26, 262205,
    33, 37, 36, 327745, 38, 39, 12, 37, 262205, 7, 40, 39, 327761, 6, 41, 40, 0, 327761, 6, 42, 40,
    1, 458832, 27, 43, 41, 42, 20, 21, 327745, 44, 45, 32, 34, 196670, 45, 43, 262205, 33, 48, 36,
    327745, 38, 49, 19, 48, 262205, 7, 50, 49, 196670, 47, 50, 65789, 65592,
];

const NATIVE_VULKAN_VULKANALIA_YCBCR_PRESENT_FRAGMENT_SPIRV: [u32; 157] = [
    119734787, 65536, 851979, 20, 0, 131089, 1, 393227, 1, 1280527431, 1685353262, 808793134, 0,
    196622, 0, 1, 458767, 4, 4, 1852399981, 0, 9, 17, 196624, 4, 7, 196611, 2, 450, 655364,
    1197427783, 1279741775, 1885560645, 1953718128, 1600482425, 1701734764, 1919509599, 1769235301,
    25974, 524292, 1197427783, 1279741775, 1852399429, 1685417059, 1768185701, 1952671090, 6649449,
    262149, 4, 1852399981, 0, 327685, 9, 1601467759, 1869377379, 114, 262149, 13, 1769365365,
    7300452, 262149, 17, 1987403638, 0, 262215, 9, 30, 0, 262215, 13, 33, 0, 262215, 13, 34, 0,
    262215, 17, 30, 0, 131091, 2, 196641, 3, 2, 196630, 6, 32, 262167, 7, 6, 4, 262176, 8, 3, 7,
    262203, 8, 9, 3, 589849, 10, 6, 1, 0, 0, 0, 1, 0, 196635, 11, 10, 262176, 12, 0, 11, 262203,
    12, 13, 0, 262167, 15, 6, 2, 262176, 16, 1, 15, 262203, 16, 17, 1, 327734, 2, 4, 0, 3, 131320,
    5, 262205, 11, 14, 13, 262205, 15, 18, 17, 327767, 7, 19, 14, 18, 196670, 9, 19, 65789, 65592,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoded_image_present_order_keeps_queue_ownership_explicit() {
        let split = native_vulkan_vulkanalia_decoded_image_present_command_order(false, "disabled");
        assert!(split.contains(&"cmd_pipeline_barrier2_video_release"));
        assert!(split.contains(&"cmd_pipeline_barrier2_graphics_acquire_shader_read"));
        assert!(split.contains(&"cmd_begin_rendering"));
        assert!(split.contains(&"cmd_pipeline_barrier2_decoded_restore"));
        assert!(split.contains(&"queue_submit2_present"));
        assert!(split.contains(&"no_queue_wait_idle_after_present"));

        let same = native_vulkan_vulkanalia_decoded_image_present_command_order(true, "disabled");
        assert!(!same.contains(&"cmd_pipeline_barrier2_video_release"));
        assert!(same.contains(&"cmd_bind_ycbcr_descriptor"));

        let present_id2 =
            native_vulkan_vulkanalia_decoded_image_present_command_order(true, "present-id2-khr");
        assert!(present_id2.contains(&"present_id2_khr"));
        assert!(
            present_id2
                .windows(2)
                .any(|pair| pair == ["present_id2_khr", "queue_present_khr"])
        );
    }

    #[test]
    fn decoded_image_present_ycbcr_model_accepts_current_video_formats() {
        assert_eq!(
            native_vulkan_vulkanalia_decoded_image_ycbcr_model(
                vk::Format::G8_B8R8_2PLANE_420_UNORM,
            )
            .unwrap(),
            vk::SamplerYcbcrModelConversion::YCBCR_709
        );
        assert_eq!(
            native_vulkan_vulkanalia_decoded_image_ycbcr_model(
                vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16,
            )
            .unwrap(),
            vk::SamplerYcbcrModelConversion::YCBCR_709
        );
        assert!(
            native_vulkan_vulkanalia_decoded_image_ycbcr_model(vk::Format::R8G8B8A8_UNORM).is_err()
        );
    }

    #[test]
    fn shader_module_code_size_uses_bytes_not_words() {
        assert_eq!(
            native_vulkan_vulkanalia_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_YCBCR_PRESENT_VERTEX_SPIRV
            ),
            NATIVE_VULKAN_VULKANALIA_YCBCR_PRESENT_VERTEX_SPIRV.len() * 4
        );
    }
}
