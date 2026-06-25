#![allow(dead_code)]

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

const SCENE_LITE_SOLID_QUAD_VERTEX_STRIDE_BYTES: u32 = 24;
const SCENE_LITE_SOLID_QUAD_PUSH_CONSTANT_BYTES: u32 = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeVulkanVulkanaliaSceneLiteDrawPassInput {
    pub(crate) plan_ready: bool,
    pub(crate) native_draw_ready: bool,
    pub(crate) draw_op_count: usize,
    pub(crate) backend_status: &'static str,
    pub(crate) blocking_reason: Option<&'static str>,
    pub(crate) fast_clear_color_ready: bool,
    pub(crate) quad_recording_ready: bool,
    pub(crate) quad_recording_step_count: usize,
    pub(crate) quad_vertex_buffer_bytes: u64,
    pub(crate) quad_index_buffer_bytes: u64,
    pub(crate) sampled_image_recording_ready: bool,
    pub(crate) sampled_image_op_count: usize,
    pub(crate) sampled_image_recording_step_count: usize,
    pub(crate) sampled_image_vertex_buffer_bytes: u64,
    pub(crate) sampled_image_index_buffer_bytes: u64,
    pub(crate) color_op_count: usize,
    pub(crate) vector_shape_op_count: usize,
    pub(crate) text_op_count: usize,
    pub(crate) path_op_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSceneLiteDrawPassSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub backend_ready: bool,
    pub backend_status: &'static str,
    pub blocking_reason: Option<&'static str>,
    pub draw_op_count: usize,
    pub color_op_count: usize,
    pub solid_quad_count: u32,
    pub sampled_image_quad_count: u32,
    pub vector_shape_op_count: usize,
    pub text_op_count: usize,
    pub path_op_count: usize,
    pub pipeline_count: u32,
    pub pipeline_labels: Vec<&'static str>,
    pub descriptor_set_count: u32,
    pub vertex_buffer_bytes: u64,
    pub index_buffer_bytes: u64,
    pub vertex_stride_bytes: u32,
    pub index_type: &'static str,
    pub draw_indexed_count: u32,
    pub render_pass_compatibility: &'static str,
    pub render_model: &'static str,
    pub command_order: Vec<&'static str>,
    pub uses_pipeline_rendering_create_info: bool,
    pub uses_dynamic_rendering: bool,
    pub uses_synchronization2: bool,
    pub uses_submit2: bool,
    pub uses_vulkan_1_4_dynamic_rendering_local_read: bool,
    pub vulkan_1_4_dynamic_rendering_local_read_policy: &'static str,
    pub zero_copy_scope: &'static str,
    pub primary_reference: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSceneLiteSolidQuadPipelineSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub target_format: String,
    pub extent: (u32, u32),
    pub shader_modules_created: bool,
    pub pipeline_layout_created: bool,
    pub pipeline_created: bool,
    pub render_pass_compatibility: &'static str,
    pub primitive_topology: &'static str,
    pub vertex_input_binding_count: u32,
    pub vertex_input_attribute_count: u32,
    pub vertex_stride_bytes: u32,
    pub vertex_position_format: &'static str,
    pub vertex_color_format: &'static str,
    pub push_constant_bytes: u32,
    pub push_constant_model: &'static str,
    pub blend_model: &'static str,
    pub uses_pipeline_rendering_create_info: bool,
    pub uses_dynamic_rendering: bool,
    pub uses_synchronization2: bool,
    pub uses_submit2: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSceneLiteSolidQuadCommandSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub extent: (u32, u32),
    pub index_count: u32,
    pub command_buffer_recorded: bool,
    pub vertex_buffer_bound: bool,
    pub index_buffer_bound: bool,
    pub push_constant_bytes: u32,
    pub swapchain_layout_transition: &'static str,
    pub render_model: &'static str,
    pub command_order: Vec<&'static str>,
    pub uses_dynamic_rendering: bool,
    pub uses_synchronization2: bool,
}

pub(super) struct VulkanaliaSceneLiteSolidQuadPipelineResources {
    pub(super) pipeline_layout: vk::PipelineLayout,
    pub(super) pipeline: vk::Pipeline,
    pub(super) snapshot: NativeVulkanVulkanaliaSceneLiteSolidQuadPipelineSnapshot,
}

pub(crate) fn native_vulkan_vulkanalia_scene_lite_draw_pass_snapshot(
    input: NativeVulkanVulkanaliaSceneLiteDrawPassInput,
) -> NativeVulkanVulkanaliaSceneLiteDrawPassSnapshot {
    let solid_quad_ready = input.plan_ready
        && input.native_draw_ready
        && input.quad_recording_ready
        && input.quad_recording_step_count == input.draw_op_count
        && input.sampled_image_op_count == 0
        && input.text_op_count == 0
        && input.path_op_count == 0;
    let sampled_image_pending = input.plan_ready
        && input.native_draw_ready
        && input.sampled_image_recording_ready
        && input.sampled_image_recording_step_count == input.sampled_image_op_count
        && input.sampled_image_op_count == input.draw_op_count;

    let (backend_ready, backend_status, blocking_reason) = if solid_quad_ready {
        (true, "solid-quad-dynamic-rendering-recording-ready", None)
    } else if !input.plan_ready || !input.native_draw_ready {
        (
            false,
            "blocked-by-scene-lite-draw-plan",
            input
                .blocking_reason
                .or(Some("scene-lite-draw-plan-not-ready")),
        )
    } else if input.fast_clear_color_ready {
        (
            false,
            "delegated-to-vulkanalia-clear-present",
            Some("fast-clear-uses-clear-present-not-draw-pass"),
        )
    } else if sampled_image_pending {
        (
            false,
            "sampled-image-dynamic-rendering-recording-pending",
            Some("sampled-image-descriptor-upload-not-yet-wired"),
        )
    } else {
        (
            false,
            input.backend_status,
            input
                .blocking_reason
                .or(Some("vulkanalia-scene-lite-recording-not-ready")),
        )
    };

    let pipeline_labels = if solid_quad_ready {
        vec!["scene-lite-solid-quad-alpha-blend"]
    } else if sampled_image_pending {
        vec!["scene-lite-sampled-image-alpha-blend-pending"]
    } else {
        Vec::new()
    };
    let descriptor_set_count = if sampled_image_pending {
        saturating_u32(input.sampled_image_op_count)
    } else {
        0
    };
    let (vertex_buffer_bytes, index_buffer_bytes, vertex_stride_bytes) = if sampled_image_pending {
        (
            input.sampled_image_vertex_buffer_bytes,
            input.sampled_image_index_buffer_bytes,
            20,
        )
    } else {
        (
            input.quad_vertex_buffer_bytes,
            input.quad_index_buffer_bytes,
            24,
        )
    };

    NativeVulkanVulkanaliaSceneLiteDrawPassSnapshot {
        binding: "vulkanalia",
        route: "scene-lite-dynamic-rendering-draw-pass",
        backend_ready,
        backend_status,
        blocking_reason,
        draw_op_count: input.draw_op_count,
        color_op_count: input.color_op_count,
        solid_quad_count: saturating_u32(input.quad_recording_step_count),
        sampled_image_quad_count: saturating_u32(input.sampled_image_recording_step_count),
        vector_shape_op_count: input.vector_shape_op_count,
        text_op_count: input.text_op_count,
        path_op_count: input.path_op_count,
        pipeline_count: saturating_u32(pipeline_labels.len()),
        pipeline_labels,
        descriptor_set_count,
        vertex_buffer_bytes,
        index_buffer_bytes,
        vertex_stride_bytes,
        index_type: "uint32",
        draw_indexed_count: if solid_quad_ready {
            saturating_u32(input.quad_recording_step_count)
        } else {
            0
        },
        render_pass_compatibility: if solid_quad_ready || sampled_image_pending {
            "dynamic-rendering-no-render-pass"
        } else {
            "not-recordable-yet"
        },
        render_model: if solid_quad_ready {
            "scene-lite solid quad vertices -> Vulkan 1.3/1.4 dynamic rendering indexed draw -> Wayland swapchain"
        } else if sampled_image_pending {
            "scene-lite image quad vertices -> sampled image descriptor upload -> dynamic rendering indexed draw"
        } else {
            "scene-lite draw pass has not reached a vulkanalia-recordable backend"
        },
        command_order: native_vulkan_vulkanalia_scene_lite_draw_pass_command_order(
            solid_quad_ready,
            sampled_image_pending,
            input.fast_clear_color_ready,
        )
        .to_vec(),
        uses_pipeline_rendering_create_info: solid_quad_ready || sampled_image_pending,
        uses_dynamic_rendering: solid_quad_ready || sampled_image_pending,
        uses_synchronization2: solid_quad_ready || sampled_image_pending,
        uses_submit2: solid_quad_ready || sampled_image_pending,
        uses_vulkan_1_4_dynamic_rendering_local_read: false,
        vulkan_1_4_dynamic_rendering_local_read_policy: "not-required-for-single-pass-solid-quad; reserve-for-multipass-scene-local-read",
        zero_copy_scope: "scene-graph-geometry-to-swapchain; no decoded-video frame copy or fallback snapshot upload",
        primary_reference: "Vulkan dynamic rendering; FFmpeg remains first reference for video clock/queue discipline",
    }
}

pub(super) fn native_vulkan_vulkanalia_create_scene_lite_solid_quad_pipeline_resources(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
) -> Result<VulkanaliaSceneLiteSolidQuadPipelineResources, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("scene-lite solid quad pipeline requires non-zero extent".to_owned());
    }

    let push_range = vk::PushConstantRange::builder()
        .stage_flags(vk::ShaderStageFlags::VERTEX)
        .offset(0)
        .size(SCENE_LITE_SOLID_QUAD_PUSH_CONSTANT_BYTES)
        .build();
    let push_ranges = [push_range];
    let pipeline_layout_info =
        vk::PipelineLayoutCreateInfo::builder().push_constant_ranges(&push_ranges);
    let pipeline_layout = unsafe { device.create_pipeline_layout(&pipeline_layout_info, None) }
        .map_err(|err| format!("vkCreatePipelineLayout(vulkanalia scene-lite quad): {err:?}"))?;

    let result = (|| -> Result<VulkanaliaSceneLiteSolidQuadPipelineResources, String> {
        let vertex_module = native_vulkan_vulkanalia_scene_lite_create_shader_module(
            device,
            &NATIVE_VULKAN_VULKANALIA_SCENE_LITE_SOLID_QUAD_VERTEX_SPIRV,
            "scene-lite solid quad vertex",
        )?;
        let result = (|| -> Result<VulkanaliaSceneLiteSolidQuadPipelineResources, String> {
            let fragment_module = native_vulkan_vulkanalia_scene_lite_create_shader_module(
                device,
                &NATIVE_VULKAN_VULKANALIA_SCENE_LITE_SOLID_QUAD_FRAGMENT_SPIRV,
                "scene-lite solid quad fragment",
            )?;
            let result = (|| -> Result<VulkanaliaSceneLiteSolidQuadPipelineResources, String> {
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
                let binding = vk::VertexInputBindingDescription::builder()
                    .binding(0)
                    .stride(SCENE_LITE_SOLID_QUAD_VERTEX_STRIDE_BYTES)
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
                        .format(vk::Format::R32G32B32A32_SFLOAT)
                        .offset(8)
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
                    format!("vkCreateGraphicsPipelines(vulkanalia scene-lite quad): {err:?}")
                })?;
                let pipeline = pipelines[0];
                Ok(VulkanaliaSceneLiteSolidQuadPipelineResources {
                    pipeline_layout,
                    pipeline,
                    snapshot: native_vulkan_vulkanalia_scene_lite_solid_quad_pipeline_snapshot(
                        target_format,
                        extent,
                    ),
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

pub(super) fn native_vulkan_vulkanalia_destroy_scene_lite_solid_quad_pipeline_resources(
    device: &Device,
    resources: VulkanaliaSceneLiteSolidQuadPipelineResources,
) {
    unsafe {
        device.destroy_pipeline(resources.pipeline, None);
        device.destroy_pipeline_layout(resources.pipeline_layout, None);
    }
}

pub(super) fn native_vulkan_vulkanalia_scene_lite_solid_quad_pipeline_snapshot(
    target_format: vk::Format,
    extent: vk::Extent2D,
) -> NativeVulkanVulkanaliaSceneLiteSolidQuadPipelineSnapshot {
    NativeVulkanVulkanaliaSceneLiteSolidQuadPipelineSnapshot {
        binding: "vulkanalia",
        route: "scene-lite-solid-quad-dynamic-rendering-pipeline",
        target_format: format!("{target_format:?}"),
        extent: (extent.width, extent.height),
        shader_modules_created: true,
        pipeline_layout_created: true,
        pipeline_created: true,
        render_pass_compatibility: "dynamic-rendering-no-render-pass",
        primitive_topology: "triangle-list-indexed-quad",
        vertex_input_binding_count: 1,
        vertex_input_attribute_count: 2,
        vertex_stride_bytes: SCENE_LITE_SOLID_QUAD_VERTEX_STRIDE_BYTES,
        vertex_position_format: "R32G32_SFLOAT",
        vertex_color_format: "R32G32B32A32_SFLOAT",
        push_constant_bytes: SCENE_LITE_SOLID_QUAD_PUSH_CONSTANT_BYTES,
        push_constant_model: "scene-space pixel extent -> NDC conversion in vertex shader",
        blend_model: "src-alpha over one-minus-src-alpha",
        uses_pipeline_rendering_create_info: true,
        uses_dynamic_rendering: true,
        uses_synchronization2: true,
        uses_submit2: true,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn native_vulkan_vulkanalia_record_scene_lite_solid_quad_command_buffer(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    swapchain_image: vk::Image,
    swapchain_view: vk::ImageView,
    extent: vk::Extent2D,
    pipeline_resources: &VulkanaliaSceneLiteSolidQuadPipelineResources,
    vertex_buffer: vk::Buffer,
    index_buffer: vk::Buffer,
    index_count: u32,
) -> Result<NativeVulkanVulkanaliaSceneLiteSolidQuadCommandSnapshot, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("scene-lite solid quad command requires non-zero extent".to_owned());
    }
    if index_count == 0 {
        return Err("scene-lite solid quad command requires at least one index".to_owned());
    }

    unsafe {
        device
            .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
            .map_err(|err| format!("vkResetCommandBuffer(vulkanalia scene-lite quad): {err:?}"))?;
        let begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
            .build();
        device
            .begin_command_buffer(command_buffer, &begin_info)
            .map_err(|err| format!("vkBeginCommandBuffer(vulkanalia scene-lite quad): {err:?}"))?;

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
            .subresource_range(native_vulkan_vulkanalia_scene_lite_color_subresource_range())
            .build();
        let image_barriers = [swapchain_to_attachment];
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
        device.cmd_bind_pipeline(
            command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            pipeline_resources.pipeline,
        );
        let vertex_buffers = [vertex_buffer];
        let vertex_offsets = [0u64];
        device.cmd_bind_vertex_buffers(command_buffer, 0, &vertex_buffers, &vertex_offsets);
        device.cmd_bind_index_buffer(command_buffer, index_buffer, 0, vk::IndexType::UINT32);
        let push_constants = [extent.width as f32, extent.height as f32];
        let push_constant_bytes = std::slice::from_raw_parts(
            push_constants.as_ptr().cast::<u8>(),
            SCENE_LITE_SOLID_QUAD_PUSH_CONSTANT_BYTES as usize,
        );
        device.cmd_push_constants(
            command_buffer,
            pipeline_resources.pipeline_layout,
            vk::ShaderStageFlags::VERTEX,
            0,
            push_constant_bytes,
        );
        device.cmd_draw_indexed(command_buffer, index_count, 1, 0, 0, 0);
        device.cmd_end_rendering(command_buffer);

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
            .subresource_range(native_vulkan_vulkanalia_scene_lite_color_subresource_range())
            .build();
        let present_barriers = [swapchain_to_present];
        let present_dependency = vk::DependencyInfo::builder()
            .image_memory_barriers(&present_barriers)
            .build();
        device.cmd_pipeline_barrier2(command_buffer, &present_dependency);

        device
            .end_command_buffer(command_buffer)
            .map_err(|err| format!("vkEndCommandBuffer(vulkanalia scene-lite quad): {err:?}"))?;
    }

    Ok(NativeVulkanVulkanaliaSceneLiteSolidQuadCommandSnapshot {
        binding: "vulkanalia",
        route: "scene-lite-solid-quad-dynamic-rendering-command-buffer",
        extent: (extent.width, extent.height),
        index_count,
        command_buffer_recorded: true,
        vertex_buffer_bound: true,
        index_buffer_bound: true,
        push_constant_bytes: SCENE_LITE_SOLID_QUAD_PUSH_CONSTANT_BYTES,
        swapchain_layout_transition: "undefined -> color-attachment-optimal -> present-src-khr",
        render_model: "scene-lite solid quad vertex/index buffers -> dynamic rendering indexed draw -> Wayland swapchain",
        command_order: native_vulkan_vulkanalia_scene_lite_draw_pass_command_order(
            true, false, false,
        )
        .to_vec(),
        uses_dynamic_rendering: true,
        uses_synchronization2: true,
    })
}

fn native_vulkan_vulkanalia_scene_lite_draw_pass_command_order(
    solid_quad_ready: bool,
    sampled_image_pending: bool,
    fast_clear_color_ready: bool,
) -> &'static [&'static str] {
    if solid_quad_ready {
        &[
            "cmd_pipeline_barrier2_swapchain_attachment",
            "cmd_begin_rendering",
            "cmd_bind_scene_lite_solid_quad_pipeline",
            "cmd_bind_scene_lite_vertex_buffer",
            "cmd_bind_scene_lite_index_buffer",
            "cmd_draw_indexed_per_quad",
            "cmd_end_rendering",
            "cmd_pipeline_barrier2_present",
            "queue_submit2_present",
            "queue_present_khr",
        ]
    } else if sampled_image_pending {
        &[
            "create_sampled_image_descriptors_pending",
            "cmd_begin_rendering_pending",
            "cmd_draw_indexed_sampled_image_quad_pending",
        ]
    } else if fast_clear_color_ready {
        &["delegate_to_vulkanalia_clear_present"]
    } else {
        &["wait_for_scene_lite_recordable_draw_ops"]
    }
}

fn saturating_u32(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
}

fn native_vulkan_vulkanalia_scene_lite_create_shader_module(
    device: &Device,
    code: &[u32],
    label: &'static str,
) -> Result<vk::ShaderModule, String> {
    if code.first().copied() != Some(0x0723_0203) {
        return Err(format!("{label} shader is not valid SPIR-V bytecode"));
    }
    let create_info = vk::ShaderModuleCreateInfo::builder().code(code).code_size(
        native_vulkan_vulkanalia_scene_lite_shader_code_size_bytes(code),
    );
    unsafe { device.create_shader_module(&create_info, None) }
        .map_err(|err| format!("vkCreateShaderModule(vulkanalia {label}): {err:?}"))
}

fn native_vulkan_vulkanalia_scene_lite_shader_code_size_bytes(code: &[u32]) -> usize {
    std::mem::size_of_val(code)
}

fn native_vulkan_vulkanalia_scene_lite_color_subresource_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1)
        .build()
}

const NATIVE_VULKAN_VULKANALIA_SCENE_LITE_SOLID_QUAD_VERTEX_SPIRV: [u32; 379] = [
    119734787, 65536, 524299, 54, 0, 131089, 1, 393227, 1, 1280527431, 1685353262, 808793134, 0,
    196622, 0, 1, 589839, 0, 4, 1852399981, 0, 11, 42, 50, 52, 196611, 2, 450, 262149, 4,
    1852399981, 0, 327685, 9, 1836216174, 2053729377, 25701, 262149, 11, 1885302377, 29551, 393221,
    13, 1852138323, 1953057893, 1937068133, 104, 327686, 13, 0, 1702131813, 29806, 196613, 15,
    25456, 196613, 22, 6513774, 393221, 40, 1348430951, 1700164197, 2019914866, 0, 393222, 40, 0,
    1348430951, 1953067887, 7237481, 458758, 40, 1, 1348430951, 1953393007, 1702521171, 0, 458758,
    40, 2, 1130327143, 1148217708, 1635021673, 6644590, 458758, 40, 3, 1130327143, 1147956341,
    1635021673, 6644590, 196613, 42, 0, 262149, 50, 1868783478, 7499628, 327685, 52, 1667198569,
    1919904879, 0, 262215, 11, 30, 0, 196679, 13, 2, 327752, 13, 0, 35, 0, 196679, 40, 2, 327752,
    40, 0, 11, 0, 327752, 40, 1, 11, 1, 327752, 40, 2, 11, 3, 327752, 40, 3, 11, 4, 262215, 50, 30,
    0, 262215, 52, 30, 1, 131091, 2, 196641, 3, 2, 196630, 6, 32, 262167, 7, 6, 2, 262176, 8, 7, 7,
    262176, 10, 1, 7, 262203, 10, 11, 1, 196638, 13, 7, 262176, 14, 9, 13, 262203, 14, 15, 9,
    262165, 16, 32, 1, 262187, 16, 17, 0, 262176, 18, 9, 7, 262165, 23, 32, 0, 262187, 23, 24, 0,
    262176, 25, 7, 6, 262187, 6, 28, 1073741824, 262187, 6, 30, 1065353216, 262187, 23, 32, 1,
    262167, 38, 6, 4, 262172, 39, 6, 32, 393246, 40, 38, 6, 39, 39, 262176, 41, 3, 40, 262203, 41,
    42, 3, 262187, 6, 44, 0, 262176, 48, 3, 38, 262203, 48, 50, 3, 262176, 51, 1, 38, 262203, 51,
    52, 1, 327734, 2, 4, 0, 3, 131320, 5, 262203, 8, 9, 7, 262203, 8, 22, 7, 262205, 7, 12, 11,
    327745, 18, 19, 15, 17, 262205, 7, 20, 19, 327816, 7, 21, 12, 20, 196670, 9, 21, 327745, 25,
    26, 9, 24, 262205, 6, 27, 26, 327813, 6, 29, 27, 28, 327811, 6, 31, 29, 30, 327745, 25, 33, 9,
    32, 262205, 6, 34, 33, 327813, 6, 35, 34, 28, 327811, 6, 36, 30, 35, 327760, 7, 37, 31, 36,
    196670, 22, 37, 262205, 7, 43, 22, 327761, 6, 45, 43, 0, 327761, 6, 46, 43, 1, 458832, 38, 47,
    45, 46, 44, 30, 327745, 48, 49, 42, 17, 196670, 49, 47, 262205, 38, 53, 52, 196670, 50, 53,
    65789, 65592,
];

const NATIVE_VULKAN_VULKANALIA_SCENE_LITE_SOLID_QUAD_FRAGMENT_SPIRV: [u32; 94] = [
    119734787, 65536, 524299, 13, 0, 131089, 1, 393227, 1, 1280527431, 1685353262, 808793134, 0,
    196622, 0, 1, 458767, 4, 4, 1852399981, 0, 9, 11, 196624, 4, 7, 196611, 2, 450, 262149, 4,
    1852399981, 0, 327685, 9, 1601467759, 1869377379, 114, 262149, 11, 1868783478, 7499628, 262215,
    9, 30, 0, 262215, 11, 30, 0, 131091, 2, 196641, 3, 2, 196630, 6, 32, 262167, 7, 6, 4, 262176,
    8, 3, 7, 262203, 8, 9, 3, 262176, 10, 1, 7, 262203, 10, 11, 1, 327734, 2, 4, 0, 3, 131320, 5,
    262205, 7, 12, 11, 196670, 9, 12, 65789, 65592,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> NativeVulkanVulkanaliaSceneLiteDrawPassInput {
        NativeVulkanVulkanaliaSceneLiteDrawPassInput {
            plan_ready: true,
            native_draw_ready: true,
            draw_op_count: 1,
            backend_status: "solid-quad-recording-ready",
            blocking_reason: None,
            fast_clear_color_ready: false,
            quad_recording_ready: true,
            quad_recording_step_count: 1,
            quad_vertex_buffer_bytes: 96,
            quad_index_buffer_bytes: 24,
            sampled_image_recording_ready: false,
            sampled_image_op_count: 0,
            sampled_image_recording_step_count: 0,
            sampled_image_vertex_buffer_bytes: 0,
            sampled_image_index_buffer_bytes: 0,
            color_op_count: 0,
            vector_shape_op_count: 1,
            text_op_count: 0,
            path_op_count: 0,
        }
    }

    #[test]
    fn solid_quad_scene_lite_path_is_dynamic_rendering_recordable() {
        let snapshot = native_vulkan_vulkanalia_scene_lite_draw_pass_snapshot(input());

        assert!(snapshot.backend_ready);
        assert_eq!(
            snapshot.backend_status,
            "solid-quad-dynamic-rendering-recording-ready"
        );
        assert_eq!(
            snapshot.pipeline_labels,
            vec!["scene-lite-solid-quad-alpha-blend"]
        );
        assert_eq!(snapshot.vertex_buffer_bytes, 96);
        assert_eq!(snapshot.index_buffer_bytes, 24);
        assert_eq!(snapshot.vertex_stride_bytes, 24);
        assert_eq!(snapshot.draw_indexed_count, 1);
        assert!(snapshot.uses_dynamic_rendering);
        assert!(snapshot.uses_pipeline_rendering_create_info);
        assert!(snapshot.uses_synchronization2);
        assert!(snapshot.uses_submit2);
        assert!(
            !snapshot.uses_vulkan_1_4_dynamic_rendering_local_read,
            "single-pass solid quads should not require local read"
        );
        assert!(snapshot.command_order.contains(&"cmd_begin_rendering"));
        assert!(
            snapshot
                .command_order
                .contains(&"cmd_draw_indexed_per_quad")
        );
    }

    #[test]
    fn sampled_image_scene_lite_path_stays_explicitly_pending() {
        let mut input = input();
        input.draw_op_count = 1;
        input.backend_status = "sampled-image-quad-payload-ready-recording-pending";
        input.quad_recording_ready = false;
        input.quad_recording_step_count = 0;
        input.quad_vertex_buffer_bytes = 0;
        input.quad_index_buffer_bytes = 0;
        input.sampled_image_recording_ready = true;
        input.sampled_image_op_count = 1;
        input.sampled_image_recording_step_count = 1;
        input.sampled_image_vertex_buffer_bytes = 80;
        input.sampled_image_index_buffer_bytes = 24;
        input.vector_shape_op_count = 0;

        let snapshot = native_vulkan_vulkanalia_scene_lite_draw_pass_snapshot(input);

        assert!(!snapshot.backend_ready);
        assert_eq!(
            snapshot.backend_status,
            "sampled-image-dynamic-rendering-recording-pending"
        );
        assert_eq!(
            snapshot.blocking_reason,
            Some("sampled-image-descriptor-upload-not-yet-wired")
        );
        assert_eq!(
            snapshot.pipeline_labels,
            vec!["scene-lite-sampled-image-alpha-blend-pending"]
        );
        assert_eq!(snapshot.descriptor_set_count, 1);
        assert_eq!(snapshot.vertex_stride_bytes, 20);
        assert_eq!(snapshot.draw_indexed_count, 0);
    }

    #[test]
    fn solid_quad_pipeline_template_uses_dynamic_rendering_and_push_constants() {
        let snapshot = native_vulkan_vulkanalia_scene_lite_solid_quad_pipeline_snapshot(
            vk::Format::B8G8R8A8_SRGB,
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
        );

        assert_eq!(snapshot.target_format, "B8G8R8A8_SRGB");
        assert_eq!(snapshot.extent, (3840, 2160));
        assert_eq!(
            snapshot.render_pass_compatibility,
            "dynamic-rendering-no-render-pass"
        );
        assert_eq!(snapshot.vertex_input_binding_count, 1);
        assert_eq!(snapshot.vertex_input_attribute_count, 2);
        assert_eq!(snapshot.vertex_stride_bytes, 24);
        assert_eq!(snapshot.push_constant_bytes, 8);
        assert_eq!(
            snapshot.push_constant_model,
            "scene-space pixel extent -> NDC conversion in vertex shader"
        );
        assert!(snapshot.uses_pipeline_rendering_create_info);
        assert!(snapshot.uses_dynamic_rendering);
    }

    #[test]
    fn solid_quad_shader_bytecode_is_inline_spirv() {
        assert_eq!(
            NATIVE_VULKAN_VULKANALIA_SCENE_LITE_SOLID_QUAD_VERTEX_SPIRV[0],
            0x0723_0203
        );
        assert_eq!(
            NATIVE_VULKAN_VULKANALIA_SCENE_LITE_SOLID_QUAD_FRAGMENT_SPIRV[0],
            0x0723_0203
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_lite_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_LITE_SOLID_QUAD_VERTEX_SPIRV
            ),
            1516
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_lite_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_LITE_SOLID_QUAD_FRAGMENT_SPIRV
            ),
            376
        );
    }

    #[test]
    fn solid_quad_command_order_records_dynamic_rendering_draw_indexed() {
        let order = native_vulkan_vulkanalia_scene_lite_draw_pass_command_order(true, false, false);

        assert_eq!(order[0], "cmd_pipeline_barrier2_swapchain_attachment");
        assert!(order.contains(&"cmd_begin_rendering"));
        assert!(order.contains(&"cmd_bind_scene_lite_solid_quad_pipeline"));
        assert!(order.contains(&"cmd_bind_scene_lite_vertex_buffer"));
        assert!(order.contains(&"cmd_bind_scene_lite_index_buffer"));
        assert!(order.contains(&"cmd_draw_indexed_per_quad"));
        assert!(order.contains(&"queue_submit2_present"));
        assert!(order.contains(&"queue_present_khr"));
    }
}
