#![allow(dead_code)]

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, ExtDescriptorHeapExtensionDeviceCommands, HasBuilder};

use super::descriptor_heap::{
    NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    VulkanaliaDescriptorHeapImageSamplerResources,
    native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_bind_info,
    native_vulkan_vulkanalia_descriptor_heap_sampler_bind_info,
};

const SCENE_FULL_SOLID_QUAD_VERTEX_STRIDE_BYTES: u32 = 24;
const SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES: u32 = 20;
const SCENE_FULL_SOLID_QUAD_PUSH_CONSTANT_BYTES: u32 = 8;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_CONSTANT_BYTES: u32 = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NativeVulkanVulkanaliaSceneDrawPassInput {
    pub(crate) plan_ready: bool,
    pub(crate) native_draw_ready: bool,
    pub(crate) draw_op_count: usize,
    pub(crate) backend_status: &'static str,
    pub(crate) blocking_reason: Option<&'static str>,
    pub(crate) fast_clear_color_ready: bool,
    pub(crate) clear_background_op_count: usize,
    pub(crate) quad_recording_ready: bool,
    pub(crate) quad_recording_step_count: usize,
    pub(crate) quad_vertex_buffer_bytes: u64,
    pub(crate) quad_index_buffer_bytes: u64,
    pub(crate) sampled_image_recording_ready: bool,
    pub(crate) sampled_image_implicit_full_extent_ready: bool,
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
pub struct NativeVulkanVulkanaliaSceneDrawPassSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub backend_ready: bool,
    pub backend_status: &'static str,
    pub blocking_reason: Option<&'static str>,
    pub draw_op_count: usize,
    pub color_op_count: usize,
    pub clear_background_op_count: usize,
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
pub struct NativeVulkanVulkanaliaSceneSolidQuadPipelineSnapshot {
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
pub struct NativeVulkanVulkanaliaSceneSolidQuadCommandSnapshot {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSceneSampledImagePipelineSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub target_format: String,
    pub extent: (u32, u32),
    pub shader_modules_created: bool,
    pub descriptor_set_layout_created: bool,
    pub pipeline_layout_created: bool,
    pub pipeline_created: bool,
    pub render_pass_compatibility: &'static str,
    pub primitive_topology: &'static str,
    pub vertex_input_binding_count: u32,
    pub vertex_input_attribute_count: u32,
    pub vertex_stride_bytes: u32,
    pub vertex_position_format: &'static str,
    pub vertex_uv_format: &'static str,
    pub vertex_opacity_format: &'static str,
    pub descriptor_set_count: u32,
    pub descriptor_model: &'static str,
    pub descriptor_heap_mapping_enabled: bool,
    pub descriptor_heap_pipeline_flag_enabled: bool,
    pub descriptor_set_layout_create_flags: Vec<&'static str>,
    pub descriptor_type: &'static str,
    pub descriptor_binding: u32,
    pub push_constant_bytes: u32,
    pub push_constant_model: &'static str,
    pub blend_model: &'static str,
    pub sampled_image_model: &'static str,
    pub uses_pipeline_rendering_create_info: bool,
    pub uses_dynamic_rendering: bool,
    pub uses_synchronization2: bool,
    pub uses_submit2: bool,
    pub uses_push_descriptor_fast_path: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanVulkanaliaSceneSampledImageCommandSnapshot {
    pub binding: &'static str,
    pub route: &'static str,
    pub extent: (u32, u32),
    pub index_count: u32,
    pub command_buffer_recorded: bool,
    pub vertex_buffer_bound: bool,
    pub index_buffer_bound: bool,
    pub draw_call_count: u32,
    pub solid_quad_draw_call_count: u32,
    pub sampled_image_draw_call_count: u32,
    pub pipeline_bind_count: u32,
    pub descriptor_set_bound: bool,
    pub push_descriptor_set_recorded: bool,
    pub descriptor_heap_bound: bool,
    pub descriptor_set_bind_count: u32,
    pub push_descriptor_set_recorded_count: u32,
    pub descriptor_heap_draw_count: u32,
    pub descriptor_model: &'static str,
    pub push_constant_bytes: u32,
    pub swapchain_layout_transition: &'static str,
    pub sampled_image_layout: &'static str,
    pub render_model: &'static str,
    pub command_order: Vec<&'static str>,
    pub uses_dynamic_rendering: bool,
    pub uses_synchronization2: bool,
}

pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneSolidQuadPipelineResources {
    pub(in crate::renderer::native_vulkan::vulkan) pipeline_layout: vk::PipelineLayout,
    pub(in crate::renderer::native_vulkan::vulkan) pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) snapshot:
        NativeVulkanVulkanaliaSceneSolidQuadPipelineSnapshot,
}

pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneSampledImagePipelineResources {
    pub(in crate::renderer::native_vulkan::vulkan) pipeline_layout: vk::PipelineLayout,
    pub(in crate::renderer::native_vulkan::vulkan) pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) snapshot:
        NativeVulkanVulkanaliaSceneSampledImagePipelineSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::vulkan) enum VulkanaliaSceneSampledImageDescriptorBinding {
    DescriptorHeap { resource_index: u32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneSampledImageDrawCommand {
    pub(in crate::renderer::native_vulkan::vulkan) layer_index: usize,
    pub(in crate::renderer::native_vulkan::vulkan) descriptor_binding:
        VulkanaliaSceneSampledImageDescriptorBinding,
    pub(in crate::renderer::native_vulkan::vulkan) first_index: u32,
    pub(in crate::renderer::native_vulkan::vulkan) index_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneSolidQuadDrawCommand {
    pub(in crate::renderer::native_vulkan::vulkan) layer_index: usize,
    pub(in crate::renderer::native_vulkan::vulkan) first_index: u32,
    pub(in crate::renderer::native_vulkan::vulkan) index_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VulkanaliaSceneOrderedDrawPipeline {
    SolidQuad,
    SampledImage,
}

impl VulkanaliaSceneOrderedDrawPipeline {
    fn sort_rank(self) -> u8 {
        match self {
            Self::SolidQuad => 0,
            Self::SampledImage => 1,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::SolidQuad => "solid-quad",
            Self::SampledImage => "sampled-image",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VulkanaliaSceneOrderedDrawStep {
    layer_index: usize,
    pipeline: VulkanaliaSceneOrderedDrawPipeline,
    command_index: usize,
}

#[derive(Clone, Copy)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneSolidQuadDrawResources<'a> {
    pub(in crate::renderer::native_vulkan::vulkan) pipeline_resources:
        &'a VulkanaliaSceneSolidQuadPipelineResources,
    pub(in crate::renderer::native_vulkan::vulkan) vertex_buffer: vk::Buffer,
    pub(in crate::renderer::native_vulkan::vulkan) index_buffer: vk::Buffer,
    pub(in crate::renderer::native_vulkan::vulkan) draw_commands:
        &'a [VulkanaliaSceneSolidQuadDrawCommand],
}

#[derive(Clone, Copy)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneDescriptorHeapDrawResources<'a>
{
    pub(in crate::renderer::native_vulkan::vulkan) resources:
        &'a VulkanaliaDescriptorHeapImageSamplerResources,
}

fn native_vulkan_vulkanalia_scene_ordered_draw_steps(
    solid_commands: &[VulkanaliaSceneSolidQuadDrawCommand],
    sampled_commands: &[VulkanaliaSceneSampledImageDrawCommand],
) -> Vec<VulkanaliaSceneOrderedDrawStep> {
    let mut ordered =
        Vec::with_capacity(solid_commands.len().saturating_add(sampled_commands.len()));
    for (command_index, command) in solid_commands.iter().enumerate() {
        ordered.push(VulkanaliaSceneOrderedDrawStep {
            layer_index: command.layer_index,
            pipeline: VulkanaliaSceneOrderedDrawPipeline::SolidQuad,
            command_index,
        });
    }
    for (command_index, command) in sampled_commands.iter().enumerate() {
        ordered.push(VulkanaliaSceneOrderedDrawStep {
            layer_index: command.layer_index,
            pipeline: VulkanaliaSceneOrderedDrawPipeline::SampledImage,
            command_index,
        });
    }
    ordered.sort_by(|left, right| {
        left.layer_index
            .cmp(&right.layer_index)
            .then(left.pipeline.sort_rank().cmp(&right.pipeline.sort_rank()))
            .then(left.command_index.cmp(&right.command_index))
    });
    ordered
}

pub(crate) fn native_vulkan_vulkanalia_scene_draw_pass_snapshot(
    input: NativeVulkanVulkanaliaSceneDrawPassInput,
) -> NativeVulkanVulkanaliaSceneDrawPassSnapshot {
    let solid_quad_ready = input.plan_ready
        && input.native_draw_ready
        && input.quad_recording_ready
        && input
            .quad_recording_step_count
            .saturating_add(input.clear_background_op_count)
            == input.draw_op_count
        && input.sampled_image_op_count == 0;
    let sampled_image_pending = input.plan_ready
        && input.native_draw_ready
        && input.sampled_image_recording_ready
        && input.sampled_image_recording_step_count == input.sampled_image_op_count
        && input
            .sampled_image_op_count
            .saturating_add(input.clear_background_op_count)
            == input.draw_op_count;
    let sampled_image_implicit_full_extent_ready = input.plan_ready
        && input.native_draw_ready
        && input.sampled_image_implicit_full_extent_ready
        && input
            .sampled_image_op_count
            .saturating_add(input.clear_background_op_count)
            == input.draw_op_count;
    let mixed_quad_sampled_image_implicit_full_extent_ready = input.plan_ready
        && input.native_draw_ready
        && input.sampled_image_implicit_full_extent_ready
        && input.quad_recording_step_count > 0
        && input.sampled_image_op_count == 1
        && input
            .quad_recording_step_count
            .saturating_add(input.sampled_image_op_count)
            .saturating_add(input.clear_background_op_count)
            == input.draw_op_count;
    let mixed_quad_sampled_image_ready = input.plan_ready
        && input.native_draw_ready
        && input.quad_recording_step_count > 0
        && input.sampled_image_recording_ready
        && input.sampled_image_recording_step_count == input.sampled_image_op_count
        && input
            .quad_recording_step_count
            .saturating_add(input.sampled_image_recording_step_count)
            .saturating_add(input.clear_background_op_count)
            == input.draw_op_count;

    let (backend_ready, backend_status, blocking_reason) = if solid_quad_ready {
        if input.clear_background_op_count > 0 {
            (
                true,
                "clear-background-solid-quad-dynamic-rendering-recording-ready",
                None,
            )
        } else {
            (true, "solid-quad-dynamic-rendering-recording-ready", None)
        }
    } else if mixed_quad_sampled_image_ready {
        if input.clear_background_op_count > 0 {
            (
                true,
                "clear-background-mixed-quad-sampled-image-dynamic-rendering-recording-ready",
                None,
            )
        } else {
            (
                true,
                "mixed-quad-sampled-image-dynamic-rendering-recording-ready",
                None,
            )
        }
    } else if mixed_quad_sampled_image_implicit_full_extent_ready {
        if input.clear_background_op_count > 0 {
            (
                true,
                "clear-background-mixed-quad-sampled-image-implicit-full-extent-present-ready",
                None,
            )
        } else {
            (
                true,
                "mixed-quad-sampled-image-implicit-full-extent-present-ready",
                None,
            )
        }
    } else if sampled_image_implicit_full_extent_ready {
        if input.clear_background_op_count > 0 {
            (
                true,
                "clear-background-sampled-image-implicit-full-extent-present-ready",
                None,
            )
        } else {
            (
                true,
                "sampled-image-implicit-full-extent-present-ready",
                None,
            )
        }
    } else if sampled_image_pending {
        if input.clear_background_op_count > 0 {
            (
                true,
                "clear-background-sampled-image-dynamic-rendering-recording-ready",
                None,
            )
        } else {
            (
                true,
                "sampled-image-dynamic-rendering-recording-ready",
                None,
            )
        }
    } else if !input.plan_ready || !input.native_draw_ready {
        (
            false,
            "blocked-by-scene-draw-plan",
            input.blocking_reason.or(Some("scene-draw-plan-not-ready")),
        )
    } else if input.fast_clear_color_ready {
        (
            false,
            "delegated-to-vulkanalia-clear-present",
            Some("fast-clear-uses-clear-present-not-draw-pass"),
        )
    } else {
        (
            false,
            input.backend_status,
            input
                .blocking_reason
                .or(Some("vulkanalia-scene-recording-not-ready")),
        )
    };

    let pipeline_labels = if solid_quad_ready {
        vec!["scene-solid-quad-alpha-blend"]
    } else if mixed_quad_sampled_image_ready || mixed_quad_sampled_image_implicit_full_extent_ready
    {
        vec![
            "scene-solid-quad-alpha-blend",
            "scene-sampled-image-alpha-blend",
        ]
    } else if sampled_image_pending || sampled_image_implicit_full_extent_ready {
        vec!["scene-sampled-image-alpha-blend"]
    } else {
        Vec::new()
    };
    let descriptor_set_count = 0;
    let (vertex_buffer_bytes, index_buffer_bytes, vertex_stride_bytes) =
        if mixed_quad_sampled_image_ready || mixed_quad_sampled_image_implicit_full_extent_ready {
            (
                input
                    .quad_vertex_buffer_bytes
                    .saturating_add(input.sampled_image_vertex_buffer_bytes),
                input
                    .quad_index_buffer_bytes
                    .saturating_add(input.sampled_image_index_buffer_bytes),
                0,
            )
        } else if sampled_image_pending {
            (
                input.sampled_image_vertex_buffer_bytes,
                input.sampled_image_index_buffer_bytes,
                SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES,
            )
        } else if sampled_image_implicit_full_extent_ready {
            (0, 0, SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES)
        } else {
            (
                input.quad_vertex_buffer_bytes,
                input.quad_index_buffer_bytes,
                24,
            )
        };

    NativeVulkanVulkanaliaSceneDrawPassSnapshot {
        binding: "vulkanalia",
        route: "scene-dynamic-rendering-draw-pass",
        backend_ready,
        backend_status,
        blocking_reason,
        draw_op_count: input.draw_op_count,
        color_op_count: input.color_op_count,
        clear_background_op_count: input.clear_background_op_count,
        solid_quad_count: saturating_u32(input.quad_recording_step_count),
        sampled_image_quad_count: if sampled_image_implicit_full_extent_ready
            || mixed_quad_sampled_image_implicit_full_extent_ready
        {
            saturating_u32(input.sampled_image_op_count)
        } else {
            saturating_u32(input.sampled_image_recording_step_count)
        },
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
        } else if mixed_quad_sampled_image_ready {
            saturating_u32(
                input
                    .quad_recording_step_count
                    .saturating_add(input.sampled_image_recording_step_count),
            )
        } else if mixed_quad_sampled_image_implicit_full_extent_ready {
            saturating_u32(
                input
                    .quad_recording_step_count
                    .saturating_add(input.sampled_image_op_count),
            )
        } else if sampled_image_pending {
            saturating_u32(input.sampled_image_recording_step_count)
        } else if sampled_image_implicit_full_extent_ready {
            saturating_u32(input.sampled_image_op_count)
        } else {
            0
        },
        render_pass_compatibility: if solid_quad_ready
            || sampled_image_pending
            || sampled_image_implicit_full_extent_ready
            || mixed_quad_sampled_image_implicit_full_extent_ready
            || mixed_quad_sampled_image_ready
        {
            "dynamic-rendering-no-render-pass"
        } else {
            "not-recordable-yet"
        },
        render_model: if solid_quad_ready {
            "scene solid quad vertices -> Vulkan 1.3/1.4 dynamic rendering indexed draw -> Wayland swapchain"
        } else if mixed_quad_sampled_image_ready {
            "scene solid quad buffers + retained sampled images -> Vulkan 1.4 dynamic rendering ordered draws -> Wayland swapchain"
        } else if mixed_quad_sampled_image_implicit_full_extent_ready {
            "scene solid quad buffers + extent-derived sampled-image geometry -> Vulkan 1.4 dynamic rendering ordered draws -> Wayland swapchain"
        } else if sampled_image_pending {
            "scene image quad vertices -> retained sampled image descriptor heap -> Vulkan 1.4 dynamic rendering indexed draw -> Wayland swapchain"
        } else if sampled_image_implicit_full_extent_ready {
            "scene image layer -> extent-derived sampled-image geometry -> retained sampled image descriptor heap -> Vulkan 1.4 dynamic rendering indexed draw -> Wayland swapchain"
        } else {
            "scene draw pass has not reached a vulkanalia-recordable backend"
        },
        command_order: native_vulkan_vulkanalia_scene_draw_pass_command_order(
            solid_quad_ready,
            sampled_image_pending || sampled_image_implicit_full_extent_ready,
            input.fast_clear_color_ready,
            mixed_quad_sampled_image_ready || mixed_quad_sampled_image_implicit_full_extent_ready,
        )
        .to_vec(),
        uses_pipeline_rendering_create_info: solid_quad_ready
            || sampled_image_pending
            || sampled_image_implicit_full_extent_ready
            || mixed_quad_sampled_image_implicit_full_extent_ready
            || mixed_quad_sampled_image_ready,
        uses_dynamic_rendering: solid_quad_ready
            || sampled_image_pending
            || sampled_image_implicit_full_extent_ready
            || mixed_quad_sampled_image_implicit_full_extent_ready
            || mixed_quad_sampled_image_ready,
        uses_synchronization2: solid_quad_ready
            || sampled_image_pending
            || sampled_image_implicit_full_extent_ready
            || mixed_quad_sampled_image_implicit_full_extent_ready
            || mixed_quad_sampled_image_ready,
        uses_submit2: solid_quad_ready
            || sampled_image_pending
            || sampled_image_implicit_full_extent_ready
            || mixed_quad_sampled_image_implicit_full_extent_ready
            || mixed_quad_sampled_image_ready,
        uses_vulkan_1_4_dynamic_rendering_local_read: false,
        vulkan_1_4_dynamic_rendering_local_read_policy: "not-required-for-single-pass-solid-quad; reserve-for-multipass-scene-local-read",
        zero_copy_scope: "scene-graph-geometry-to-swapchain; no decoded-video frame copy or scene snapshot upload",
        primary_reference: "Vulkan dynamic rendering; FFmpeg remains first reference for video clock/queue discipline",
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_create_scene_solid_quad_pipeline_resources(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
) -> Result<VulkanaliaSceneSolidQuadPipelineResources, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("scene solid quad pipeline requires non-zero extent".to_owned());
    }

    let push_range = vk::PushConstantRange::builder()
        .stage_flags(vk::ShaderStageFlags::VERTEX)
        .offset(0)
        .size(SCENE_FULL_SOLID_QUAD_PUSH_CONSTANT_BYTES)
        .build();
    let push_ranges = [push_range];
    let pipeline_layout_info =
        vk::PipelineLayoutCreateInfo::builder().push_constant_ranges(&push_ranges);
    let pipeline_layout = unsafe { device.create_pipeline_layout(&pipeline_layout_info, None) }
        .map_err(|err| format!("vkCreatePipelineLayout(vulkanalia scene quad): {err:?}"))?;

    let result = (|| -> Result<VulkanaliaSceneSolidQuadPipelineResources, String> {
        let vertex_module = native_vulkan_vulkanalia_scene_create_shader_module(
            device,
            &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SOLID_QUAD_VERTEX_SPIRV,
            "scene solid quad vertex",
        )?;
        let result = (|| -> Result<VulkanaliaSceneSolidQuadPipelineResources, String> {
            let fragment_module = native_vulkan_vulkanalia_scene_create_shader_module(
                device,
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SOLID_QUAD_FRAGMENT_SPIRV,
                "scene solid quad fragment",
            )?;
            let result = (|| -> Result<VulkanaliaSceneSolidQuadPipelineResources, String> {
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
                    .stride(SCENE_FULL_SOLID_QUAD_VERTEX_STRIDE_BYTES)
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
                    format!("vkCreateGraphicsPipelines(vulkanalia scene quad): {err:?}")
                })?;
                let pipeline = pipelines[0];
                Ok(VulkanaliaSceneSolidQuadPipelineResources {
                    pipeline_layout,
                    pipeline,
                    snapshot: native_vulkan_vulkanalia_scene_solid_quad_pipeline_snapshot(
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

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_destroy_scene_solid_quad_pipeline_resources(
    device: &Device,
    resources: VulkanaliaSceneSolidQuadPipelineResources,
) {
    unsafe {
        device.destroy_pipeline(resources.pipeline, None);
        device.destroy_pipeline_layout(resources.pipeline_layout, None);
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_scene_solid_quad_pipeline_snapshot(
    target_format: vk::Format,
    extent: vk::Extent2D,
) -> NativeVulkanVulkanaliaSceneSolidQuadPipelineSnapshot {
    NativeVulkanVulkanaliaSceneSolidQuadPipelineSnapshot {
        binding: "vulkanalia",
        route: "scene-solid-quad-dynamic-rendering-pipeline",
        target_format: format!("{target_format:?}"),
        extent: (extent.width, extent.height),
        shader_modules_created: true,
        pipeline_layout_created: true,
        pipeline_created: true,
        render_pass_compatibility: "dynamic-rendering-no-render-pass",
        primitive_topology: "triangle-list-indexed-quad",
        vertex_input_binding_count: 1,
        vertex_input_attribute_count: 2,
        vertex_stride_bytes: SCENE_FULL_SOLID_QUAD_VERTEX_STRIDE_BYTES,
        vertex_position_format: "R32G32_SFLOAT",
        vertex_color_format: "R32G32B32A32_SFLOAT",
        push_constant_bytes: SCENE_FULL_SOLID_QUAD_PUSH_CONSTANT_BYTES,
        push_constant_model: "scene-space pixel extent -> NDC conversion in vertex shader",
        blend_model: "src-alpha over one-minus-src-alpha",
        uses_pipeline_rendering_create_info: true,
        uses_dynamic_rendering: true,
        uses_synchronization2: true,
        uses_submit2: true,
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_create_scene_sampled_image_pipeline_resources(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
) -> Result<VulkanaliaSceneSampledImagePipelineResources, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("scene sampled-image pipeline requires non-zero extent".to_owned());
    }
    if !descriptor_heap_plan.backend_ready {
        return Err(
            "scene sampled-image pipeline requires a ready VK_EXT_descriptor_heap plan".to_owned(),
        );
    }

    let result = (|| -> Result<VulkanaliaSceneSampledImagePipelineResources, String> {
        let push_range = vk::PushConstantRange::builder()
            .stage_flags(vk::ShaderStageFlags::VERTEX)
            .offset(0)
            .size(SCENE_FULL_SAMPLED_IMAGE_PUSH_CONSTANT_BYTES)
            .build();
        let push_ranges = [push_range];
        let pipeline_layout_info =
            vk::PipelineLayoutCreateInfo::builder().push_constant_ranges(&push_ranges);
        let pipeline_layout = unsafe { device.create_pipeline_layout(&pipeline_layout_info, None) }
            .map_err(|err| {
                format!("vkCreatePipelineLayout(vulkanalia scene sampled image): {err:?}")
            })?;

        let result = (|| -> Result<VulkanaliaSceneSampledImagePipelineResources, String> {
            let vertex_module = native_vulkan_vulkanalia_scene_create_shader_module(
                device,
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_VERTEX_SPIRV,
                "scene sampled image vertex",
            )?;
            let result = (|| -> Result<VulkanaliaSceneSampledImagePipelineResources, String> {
                let fragment_module = native_vulkan_vulkanalia_scene_create_shader_module(
                    device,
                    &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_FRAGMENT_SPIRV,
                    "scene sampled image fragment",
                )?;
                let result =
                    (|| -> Result<VulkanaliaSceneSampledImagePipelineResources, String> {
                        let shader_entry = b"main\0";
                        let descriptor_heap_mapping =
                                native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_mapping(
                                    descriptor_heap_plan,
                                    0,
                                )?;
                        let descriptor_heap_mappings = [descriptor_heap_mapping];
                        let mut descriptor_heap_mapping_info =
                            vk::ShaderDescriptorSetAndBindingMappingInfoEXT::builder()
                                .mappings(&descriptor_heap_mappings)
                                .build();
                        let mut fragment_stage = vk::PipelineShaderStageCreateInfo::builder()
                            .stage(vk::ShaderStageFlags::FRAGMENT)
                            .module(fragment_module)
                            .name(shader_entry);
                        fragment_stage =
                            fragment_stage.push_next(&mut descriptor_heap_mapping_info);
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
                            .stride(SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES)
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
                                "vkCreateGraphicsPipelines(vulkanalia scene sampled image): {err:?}"
                            )
                        })?;
                        let pipeline = pipelines[0];
                        Ok(VulkanaliaSceneSampledImagePipelineResources {
                            pipeline_layout,
                            pipeline,
                            snapshot:
                                native_vulkan_vulkanalia_scene_sampled_image_pipeline_snapshot(
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
    })();

    result
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_destroy_scene_sampled_image_pipeline_resources(
    device: &Device,
    resources: VulkanaliaSceneSampledImagePipelineResources,
) {
    unsafe {
        device.destroy_pipeline(resources.pipeline, None);
        device.destroy_pipeline_layout(resources.pipeline_layout, None);
    }
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_scene_sampled_image_pipeline_snapshot(
    target_format: vk::Format,
    extent: vk::Extent2D,
) -> NativeVulkanVulkanaliaSceneSampledImagePipelineSnapshot {
    NativeVulkanVulkanaliaSceneSampledImagePipelineSnapshot {
        binding: "vulkanalia",
        route: "scene-sampled-image-dynamic-rendering-pipeline",
        target_format: format!("{target_format:?}"),
        extent: (extent.width, extent.height),
        shader_modules_created: true,
        descriptor_set_layout_created: false,
        pipeline_layout_created: true,
        pipeline_created: true,
        render_pass_compatibility: "dynamic-rendering-no-render-pass",
        primitive_topology: "triangle-list-indexed-image-quad",
        vertex_input_binding_count: 1,
        vertex_input_attribute_count: 3,
        vertex_stride_bytes: SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES,
        vertex_position_format: "R32G32_SFLOAT",
        vertex_uv_format: "R32G32_SFLOAT",
        vertex_opacity_format: "R32_SFLOAT",
        descriptor_set_count: 0,
        descriptor_model: "VK_EXT_descriptor_heap",
        descriptor_heap_mapping_enabled: true,
        descriptor_heap_pipeline_flag_enabled: true,
        descriptor_set_layout_create_flags: Vec::new(),
        descriptor_type: "combined-image-sampler",
        descriptor_binding: 0,
        push_constant_bytes: SCENE_FULL_SAMPLED_IMAGE_PUSH_CONSTANT_BYTES,
        push_constant_model: "scene-space pixel extent -> NDC conversion in vertex shader",
        blend_model: "sampled rgba with opacity; src-alpha over one-minus-src-alpha",
        sampled_image_model: "retained BC7_UNORM_BLOCK sampled image -> VK_EXT_descriptor_heap constant-offset mapping -> fragment shader",
        uses_pipeline_rendering_create_info: true,
        uses_dynamic_rendering: true,
        uses_synchronization2: true,
        uses_submit2: true,
        uses_push_descriptor_fast_path: false,
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_record_scene_solid_quad_command_buffer(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    swapchain_image: vk::Image,
    swapchain_view: vk::ImageView,
    extent: vk::Extent2D,
    pipeline_resources: &VulkanaliaSceneSolidQuadPipelineResources,
    vertex_buffer: vk::Buffer,
    index_buffer: vk::Buffer,
    index_count: u32,
    clear_color: [f32; 4],
) -> Result<NativeVulkanVulkanaliaSceneSolidQuadCommandSnapshot, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("scene solid quad command requires non-zero extent".to_owned());
    }
    if index_count == 0 {
        return Err("scene solid quad command requires at least one index".to_owned());
    }

    unsafe {
        device
            .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
            .map_err(|err| format!("vkResetCommandBuffer(vulkanalia scene quad): {err:?}"))?;
        let begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
            .build();
        device
            .begin_command_buffer(command_buffer, &begin_info)
            .map_err(|err| format!("vkBeginCommandBuffer(vulkanalia scene quad): {err:?}"))?;

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
            .subresource_range(native_vulkan_vulkanalia_scene_color_subresource_range())
            .build();
        let image_barriers = [swapchain_to_attachment];
        let dependency = vk::DependencyInfo::builder()
            .image_memory_barriers(&image_barriers)
            .build();
        device.cmd_pipeline_barrier2(command_buffer, &dependency);

        let clear_value = vk::ClearValue {
            color: vk::ClearColorValue {
                float32: clear_color,
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
            SCENE_FULL_SOLID_QUAD_PUSH_CONSTANT_BYTES as usize,
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
            .subresource_range(native_vulkan_vulkanalia_scene_color_subresource_range())
            .build();
        let present_barriers = [swapchain_to_present];
        let present_dependency = vk::DependencyInfo::builder()
            .image_memory_barriers(&present_barriers)
            .build();
        device.cmd_pipeline_barrier2(command_buffer, &present_dependency);

        device
            .end_command_buffer(command_buffer)
            .map_err(|err| format!("vkEndCommandBuffer(vulkanalia scene quad): {err:?}"))?;
    }

    Ok(NativeVulkanVulkanaliaSceneSolidQuadCommandSnapshot {
        binding: "vulkanalia",
        route: "scene-solid-quad-dynamic-rendering-command-buffer",
        extent: (extent.width, extent.height),
        index_count,
        command_buffer_recorded: true,
        vertex_buffer_bound: true,
        index_buffer_bound: true,
        push_constant_bytes: SCENE_FULL_SOLID_QUAD_PUSH_CONSTANT_BYTES,
        swapchain_layout_transition: "undefined -> color-attachment-optimal -> present-src-khr",
        render_model: "scene solid quad vertex/index buffers -> dynamic rendering indexed draw -> Wayland swapchain",
        command_order: native_vulkan_vulkanalia_scene_draw_pass_command_order(
            true, false, false, false,
        )
        .to_vec(),
        uses_dynamic_rendering: true,
        uses_synchronization2: true,
    })
}

#[allow(clippy::too_many_arguments)]
pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_record_scene_solid_quad_draws_inside_rendering(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    extent: vk::Extent2D,
    solid_quad_draw: VulkanaliaSceneSolidQuadDrawResources<'_>,
) -> Result<u32, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("scene solid draw requires non-zero extent".to_owned());
    }
    if solid_quad_draw.draw_commands.is_empty() {
        return Err("scene solid draw requires non-empty draw steps".to_owned());
    }
    for solid_draw in solid_quad_draw.draw_commands {
        if solid_draw.index_count == 0 {
            return Err("scene solid draw requires non-empty indices".to_owned());
        }
    }

    unsafe {
        let solid_push_constants = [extent.width as f32, extent.height as f32];
        let solid_push_constant_bytes = std::slice::from_raw_parts(
            solid_push_constants.as_ptr().cast::<u8>(),
            SCENE_FULL_SOLID_QUAD_PUSH_CONSTANT_BYTES as usize,
        );
        device.cmd_bind_pipeline(
            command_buffer,
            vk::PipelineBindPoint::GRAPHICS,
            solid_quad_draw.pipeline_resources.pipeline,
        );
        let vertex_buffers = [solid_quad_draw.vertex_buffer];
        let vertex_offsets = [0u64];
        device.cmd_bind_vertex_buffers(command_buffer, 0, &vertex_buffers, &vertex_offsets);
        device.cmd_bind_index_buffer(
            command_buffer,
            solid_quad_draw.index_buffer,
            0,
            vk::IndexType::UINT32,
        );
        device.cmd_push_constants(
            command_buffer,
            solid_quad_draw.pipeline_resources.pipeline_layout,
            vk::ShaderStageFlags::VERTEX,
            0,
            solid_push_constant_bytes,
        );
        for solid_draw in solid_quad_draw.draw_commands {
            device.cmd_draw_indexed(
                command_buffer,
                solid_draw.index_count,
                1,
                solid_draw.first_index,
                0,
                0,
            );
        }
    }

    Ok(solid_quad_draw
        .draw_commands
        .iter()
        .fold(0u32, |sum, draw| sum.saturating_add(draw.index_count)))
}

#[allow(clippy::too_many_arguments)]
pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_record_scene_sampled_image_draws_inside_rendering(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    extent: vk::Extent2D,
    solid_quad_draw: Option<VulkanaliaSceneSolidQuadDrawResources<'_>>,
    descriptor_heap_draw: Option<VulkanaliaSceneDescriptorHeapDrawResources<'_>>,
    pipeline_resources: &VulkanaliaSceneSampledImagePipelineResources,
    draw_commands: &[VulkanaliaSceneSampledImageDrawCommand],
    vertex_buffer: vk::Buffer,
    index_buffer: vk::Buffer,
) -> Result<u32, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("scene sampled-image draw requires non-zero extent".to_owned());
    }
    if draw_commands.is_empty() {
        return Err("scene sampled-image draw requires at least one draw".to_owned());
    }
    if let Some(draw) = solid_quad_draw {
        if draw.draw_commands.is_empty() {
            return Err("scene mixed draw requires non-empty solid draw steps".to_owned());
        }
        for solid_draw in draw.draw_commands {
            if solid_draw.index_count == 0 {
                return Err("scene mixed draw requires non-empty solid draw indices".to_owned());
            }
        }
    }
    for draw in draw_commands {
        if draw.index_count == 0 {
            return Err("scene sampled-image draw requires at least one index".to_owned());
        }
        match draw.descriptor_binding {
            VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap { resource_index } => {
                let Some(descriptor_heap_draw) = descriptor_heap_draw else {
                    return Err(
                        "scene sampled-image descriptor heap draw requires heap resources"
                            .to_owned(),
                    );
                };
                if resource_index as usize >= descriptor_heap_draw.resources.plan.image_count {
                    return Err(format!(
                        "scene sampled-image descriptor heap resource index {resource_index} exceeds heap image count {}",
                        descriptor_heap_draw.resources.plan.image_count
                    ));
                }
            }
        }
    }
    if descriptor_heap_draw.is_none() {
        return Err("scene sampled-image draw requires descriptor heap resources".to_owned());
    }

    let solid_draw_commands: &[VulkanaliaSceneSolidQuadDrawCommand] =
        solid_quad_draw.map_or(&[], |draw| draw.draw_commands);
    let ordered_draws =
        native_vulkan_vulkanalia_scene_ordered_draw_steps(solid_draw_commands, draw_commands);

    unsafe {
        let solid_push_constants = [extent.width as f32, extent.height as f32];
        let solid_push_constant_bytes = std::slice::from_raw_parts(
            solid_push_constants.as_ptr().cast::<u8>(),
            SCENE_FULL_SOLID_QUAD_PUSH_CONSTANT_BYTES as usize,
        );
        let sampled_push_constants = [extent.width as f32, extent.height as f32];
        let sampled_push_constant_bytes = std::slice::from_raw_parts(
            sampled_push_constants.as_ptr().cast::<u8>(),
            SCENE_FULL_SAMPLED_IMAGE_PUSH_CONSTANT_BYTES as usize,
        );
        let mut bound_pipeline: Option<u8> = None;
        let mut descriptor_heap_bound = false;
        for draw in &ordered_draws {
            match draw.pipeline {
                VulkanaliaSceneOrderedDrawPipeline::SolidQuad => {
                    let solid_draw = &solid_draw_commands[draw.command_index];
                    if bound_pipeline != Some(draw.pipeline.sort_rank()) {
                        let solid_resources = solid_quad_draw
                            .as_ref()
                            .expect("solid draw resources present");
                        device.cmd_bind_pipeline(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            solid_resources.pipeline_resources.pipeline,
                        );
                        let vertex_buffers = [solid_resources.vertex_buffer];
                        let vertex_offsets = [0u64];
                        device.cmd_bind_vertex_buffers(
                            command_buffer,
                            0,
                            &vertex_buffers,
                            &vertex_offsets,
                        );
                        device.cmd_bind_index_buffer(
                            command_buffer,
                            solid_resources.index_buffer,
                            0,
                            vk::IndexType::UINT32,
                        );
                        device.cmd_push_constants(
                            command_buffer,
                            solid_resources.pipeline_resources.pipeline_layout,
                            vk::ShaderStageFlags::VERTEX,
                            0,
                            solid_push_constant_bytes,
                        );
                        bound_pipeline = Some(draw.pipeline.sort_rank());
                    }
                    device.cmd_draw_indexed(
                        command_buffer,
                        solid_draw.index_count,
                        1,
                        solid_draw.first_index,
                        0,
                        0,
                    );
                }
                VulkanaliaSceneOrderedDrawPipeline::SampledImage => {
                    let sampled_draw = &draw_commands[draw.command_index];
                    if bound_pipeline != Some(draw.pipeline.sort_rank()) {
                        device.cmd_bind_pipeline(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            pipeline_resources.pipeline,
                        );
                        let vertex_buffers = [vertex_buffer];
                        let vertex_offsets = [0u64];
                        device.cmd_bind_vertex_buffers(
                            command_buffer,
                            0,
                            &vertex_buffers,
                            &vertex_offsets,
                        );
                        device.cmd_bind_index_buffer(
                            command_buffer,
                            index_buffer,
                            0,
                            vk::IndexType::UINT32,
                        );
                        device.cmd_push_constants(
                            command_buffer,
                            pipeline_resources.pipeline_layout,
                            vk::ShaderStageFlags::VERTEX,
                            0,
                            sampled_push_constant_bytes,
                        );
                        if let Some(descriptor_heap_draw) = descriptor_heap_draw {
                            if !descriptor_heap_bound {
                                let resource_bind =
                                    native_vulkan_vulkanalia_descriptor_heap_resource_bind_info(
                                        descriptor_heap_draw.resources,
                                    );
                                let sampler_bind =
                                    native_vulkan_vulkanalia_descriptor_heap_sampler_bind_info(
                                        descriptor_heap_draw.resources,
                                    );
                                device.cmd_bind_resource_heap_ext(command_buffer, &resource_bind);
                                device.cmd_bind_sampler_heap_ext(command_buffer, &sampler_bind);
                                descriptor_heap_bound = true;
                            }
                        }
                        bound_pipeline = Some(draw.pipeline.sort_rank());
                    }
                    let VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap {
                        resource_index: _,
                    } = sampled_draw.descriptor_binding;
                    let _ = descriptor_heap_draw.expect("descriptor heap draw resources present");
                    device.cmd_draw_indexed(
                        command_buffer,
                        sampled_draw.index_count,
                        1,
                        sampled_draw.first_index,
                        0,
                        0,
                    );
                }
            }
        }
    }

    let sampled_image_index_count = draw_commands
        .iter()
        .fold(0u32, |sum, draw| sum.saturating_add(draw.index_count));
    let solid_quad_index_count = solid_quad_draw.map_or(0, |draw| {
        draw.draw_commands
            .iter()
            .fold(0u32, |sum, draw| sum.saturating_add(draw.index_count))
    });
    Ok(solid_quad_index_count.saturating_add(sampled_image_index_count))
}

#[allow(clippy::too_many_arguments)]
pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_record_scene_sampled_image_command_buffer(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    swapchain_image: vk::Image,
    swapchain_view: vk::ImageView,
    extent: vk::Extent2D,
    solid_quad_draw: Option<VulkanaliaSceneSolidQuadDrawResources<'_>>,
    descriptor_heap_draw: Option<VulkanaliaSceneDescriptorHeapDrawResources<'_>>,
    pipeline_resources: &VulkanaliaSceneSampledImagePipelineResources,
    draw_commands: &[VulkanaliaSceneSampledImageDrawCommand],
    vertex_buffer: vk::Buffer,
    index_buffer: vk::Buffer,
    clear_color: [f32; 4],
) -> Result<NativeVulkanVulkanaliaSceneSampledImageCommandSnapshot, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("scene sampled-image command requires non-zero extent".to_owned());
    }
    if draw_commands.is_empty() {
        return Err("scene sampled-image command requires at least one draw".to_owned());
    }
    if let Some(draw) = solid_quad_draw {
        if draw.draw_commands.is_empty() {
            return Err("scene mixed command requires non-empty solid draw steps".to_owned());
        }
        for solid_draw in draw.draw_commands {
            if solid_draw.index_count == 0 {
                return Err("scene mixed command requires non-empty solid draw indices".to_owned());
            }
        }
    }
    for draw in draw_commands {
        if draw.index_count == 0 {
            return Err("scene sampled-image draw requires at least one index".to_owned());
        }
        match draw.descriptor_binding {
            VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap { resource_index } => {
                let Some(descriptor_heap_draw) = descriptor_heap_draw else {
                    return Err(
                        "scene sampled-image descriptor heap draw requires heap resources"
                            .to_owned(),
                    );
                };
                if resource_index as usize >= descriptor_heap_draw.resources.plan.image_count {
                    return Err(format!(
                        "scene sampled-image descriptor heap resource index {resource_index} exceeds heap image count {}",
                        descriptor_heap_draw.resources.plan.image_count
                    ));
                }
            }
        }
    }
    if descriptor_heap_draw.is_none() {
        return Err("scene sampled-image command requires descriptor heap resources".to_owned());
    }

    let solid_draw_commands: &[VulkanaliaSceneSolidQuadDrawCommand] =
        solid_quad_draw.map_or(&[], |draw| draw.draw_commands);
    let ordered_draws =
        native_vulkan_vulkanalia_scene_ordered_draw_steps(solid_draw_commands, draw_commands);

    unsafe {
        device
            .reset_command_buffer(command_buffer, vk::CommandBufferResetFlags::empty())
            .map_err(|err| {
                format!("vkResetCommandBuffer(vulkanalia scene sampled image): {err:?}")
            })?;
        let begin_info = vk::CommandBufferBeginInfo::builder()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT)
            .build();
        device
            .begin_command_buffer(command_buffer, &begin_info)
            .map_err(|err| {
                format!("vkBeginCommandBuffer(vulkanalia scene sampled image): {err:?}")
            })?;

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
            .subresource_range(native_vulkan_vulkanalia_scene_color_subresource_range())
            .build();
        let image_barriers = [swapchain_to_attachment];
        let dependency = vk::DependencyInfo::builder()
            .image_memory_barriers(&image_barriers)
            .build();
        device.cmd_pipeline_barrier2(command_buffer, &dependency);

        let clear_value = vk::ClearValue {
            color: vk::ClearColorValue {
                float32: clear_color,
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
        let solid_push_constants = [extent.width as f32, extent.height as f32];
        let solid_push_constant_bytes = std::slice::from_raw_parts(
            solid_push_constants.as_ptr().cast::<u8>(),
            SCENE_FULL_SOLID_QUAD_PUSH_CONSTANT_BYTES as usize,
        );
        let sampled_push_constants = [extent.width as f32, extent.height as f32];
        let sampled_push_constant_bytes = std::slice::from_raw_parts(
            sampled_push_constants.as_ptr().cast::<u8>(),
            SCENE_FULL_SAMPLED_IMAGE_PUSH_CONSTANT_BYTES as usize,
        );
        let mut bound_pipeline: Option<u8> = None;
        let mut descriptor_heap_bound = false;
        for draw in &ordered_draws {
            match draw.pipeline {
                VulkanaliaSceneOrderedDrawPipeline::SolidQuad => {
                    let solid_draw = &solid_draw_commands[draw.command_index];
                    if bound_pipeline != Some(draw.pipeline.sort_rank()) {
                        let solid_resources = solid_quad_draw
                            .as_ref()
                            .expect("solid draw resources present");
                        device.cmd_bind_pipeline(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            solid_resources.pipeline_resources.pipeline,
                        );
                        let vertex_buffers = [solid_resources.vertex_buffer];
                        let vertex_offsets = [0u64];
                        device.cmd_bind_vertex_buffers(
                            command_buffer,
                            0,
                            &vertex_buffers,
                            &vertex_offsets,
                        );
                        device.cmd_bind_index_buffer(
                            command_buffer,
                            solid_resources.index_buffer,
                            0,
                            vk::IndexType::UINT32,
                        );
                        device.cmd_push_constants(
                            command_buffer,
                            solid_resources.pipeline_resources.pipeline_layout,
                            vk::ShaderStageFlags::VERTEX,
                            0,
                            solid_push_constant_bytes,
                        );
                        bound_pipeline = Some(draw.pipeline.sort_rank());
                    }
                    device.cmd_draw_indexed(
                        command_buffer,
                        solid_draw.index_count,
                        1,
                        solid_draw.first_index,
                        0,
                        0,
                    );
                }
                VulkanaliaSceneOrderedDrawPipeline::SampledImage => {
                    let sampled_draw = &draw_commands[draw.command_index];
                    if bound_pipeline != Some(draw.pipeline.sort_rank()) {
                        device.cmd_bind_pipeline(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            pipeline_resources.pipeline,
                        );
                        let vertex_buffers = [vertex_buffer];
                        let vertex_offsets = [0u64];
                        device.cmd_bind_vertex_buffers(
                            command_buffer,
                            0,
                            &vertex_buffers,
                            &vertex_offsets,
                        );
                        device.cmd_bind_index_buffer(
                            command_buffer,
                            index_buffer,
                            0,
                            vk::IndexType::UINT32,
                        );
                        device.cmd_push_constants(
                            command_buffer,
                            pipeline_resources.pipeline_layout,
                            vk::ShaderStageFlags::VERTEX,
                            0,
                            sampled_push_constant_bytes,
                        );
                        if let Some(descriptor_heap_draw) = descriptor_heap_draw {
                            if !descriptor_heap_bound {
                                let resource_bind =
                                    native_vulkan_vulkanalia_descriptor_heap_resource_bind_info(
                                        descriptor_heap_draw.resources,
                                    );
                                let sampler_bind =
                                    native_vulkan_vulkanalia_descriptor_heap_sampler_bind_info(
                                        descriptor_heap_draw.resources,
                                    );
                                device.cmd_bind_resource_heap_ext(command_buffer, &resource_bind);
                                device.cmd_bind_sampler_heap_ext(command_buffer, &sampler_bind);
                                descriptor_heap_bound = true;
                            }
                        }
                        bound_pipeline = Some(draw.pipeline.sort_rank());
                    }
                    let VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap {
                        resource_index: _,
                    } = sampled_draw.descriptor_binding;
                    let _ = descriptor_heap_draw.expect("descriptor heap draw resources present");
                    device.cmd_draw_indexed(
                        command_buffer,
                        sampled_draw.index_count,
                        1,
                        sampled_draw.first_index,
                        0,
                        0,
                    );
                }
            }
        }
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
            .subresource_range(native_vulkan_vulkanalia_scene_color_subresource_range())
            .build();
        let present_barriers = [swapchain_to_present];
        let present_dependency = vk::DependencyInfo::builder()
            .image_memory_barriers(&present_barriers)
            .build();
        device.cmd_pipeline_barrier2(command_buffer, &present_dependency);

        device.end_command_buffer(command_buffer).map_err(|err| {
            format!("vkEndCommandBuffer(vulkanalia scene sampled image): {err:?}")
        })?;
    }

    let descriptor_set_bind_count = 0;
    let push_descriptor_set_recorded_count = 0;
    let descriptor_heap_draw_count = saturating_u32(
        draw_commands
            .iter()
            .filter(|draw| {
                matches!(
                    draw.descriptor_binding,
                    VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap { .. }
                )
            })
            .count(),
    );
    let sampled_image_index_count = draw_commands
        .iter()
        .fold(0u32, |sum, draw| sum.saturating_add(draw.index_count));
    let solid_quad_index_count = solid_quad_draw.map_or(0, |draw| {
        draw.draw_commands
            .iter()
            .fold(0u32, |sum, draw| sum.saturating_add(draw.index_count))
    });
    let solid_quad_draw_call_count =
        solid_quad_draw.map_or(0, |draw| saturating_u32(draw.draw_commands.len()));
    let sampled_image_draw_call_count = saturating_u32(draw_commands.len());
    let draw_call_count = solid_quad_draw_call_count.saturating_add(sampled_image_draw_call_count);
    let mut last_pipeline = None;
    let mut pipeline_bind_count = 0u32;
    for draw in &ordered_draws {
        let pipeline_rank = draw.pipeline.sort_rank();
        if last_pipeline != Some(pipeline_rank) {
            pipeline_bind_count = pipeline_bind_count.saturating_add(1);
            last_pipeline = Some(pipeline_rank);
        }
    }

    Ok(NativeVulkanVulkanaliaSceneSampledImageCommandSnapshot {
        binding: "vulkanalia",
        route: if solid_quad_draw.is_some() {
            "scene-mixed-quad-sampled-image-dynamic-rendering-command-buffer"
        } else {
            "scene-sampled-image-dynamic-rendering-command-buffer"
        },
        extent: (extent.width, extent.height),
        index_count: solid_quad_index_count.saturating_add(sampled_image_index_count),
        command_buffer_recorded: true,
        vertex_buffer_bound: true,
        index_buffer_bound: true,
        draw_call_count,
        solid_quad_draw_call_count,
        sampled_image_draw_call_count,
        pipeline_bind_count,
        descriptor_set_bound: descriptor_set_bind_count > 0,
        push_descriptor_set_recorded: push_descriptor_set_recorded_count > 0,
        descriptor_heap_bound: descriptor_heap_draw.is_some() && descriptor_heap_draw_count > 0,
        descriptor_set_bind_count,
        push_descriptor_set_recorded_count,
        descriptor_heap_draw_count,
        descriptor_model: "VK_EXT_descriptor_heap",
        push_constant_bytes: SCENE_FULL_SAMPLED_IMAGE_PUSH_CONSTANT_BYTES,
        swapchain_layout_transition: "undefined -> color-attachment-optimal -> present-src-khr",
        sampled_image_layout: "shader-read-only-optimal",
        render_model: if solid_quad_draw.is_some() {
            "scene solid quad buffers then sampled image buffers/descriptor heap -> one dynamic rendering pass -> Wayland swapchain"
        } else {
            "scene sampled image vertex/index buffers + VK_EXT_descriptor_heap combined-image-sampler mapping -> dynamic rendering indexed draw -> Wayland swapchain"
        },
        command_order: native_vulkan_vulkanalia_scene_draw_pass_command_order(
            false,
            true,
            false,
            solid_quad_draw.is_some(),
        )
        .to_vec(),
        uses_dynamic_rendering: true,
        uses_synchronization2: true,
    })
}

fn native_vulkan_vulkanalia_scene_draw_pass_command_order(
    solid_quad_ready: bool,
    sampled_image_pending: bool,
    fast_clear_color_ready: bool,
    mixed_quad_sampled_image_ready: bool,
) -> &'static [&'static str] {
    if mixed_quad_sampled_image_ready {
        &[
            "cmd_pipeline_barrier2_swapchain_attachment",
            "cmd_begin_rendering",
            "cmd_bind_scene_solid_quad_pipeline_as_needed",
            "cmd_bind_scene_sampled_image_pipeline_as_needed",
            "cmd_bind_scene_geometry_for_next_layer",
            "cmd_bind_scene_descriptor_heap_when_needed",
            "cmd_draw_indexed_in_scene_layer_order",
            "cmd_end_rendering",
            "cmd_pipeline_barrier2_present",
            "queue_submit2_present",
            "queue_present_khr",
        ]
    } else if solid_quad_ready {
        &[
            "cmd_pipeline_barrier2_swapchain_attachment",
            "cmd_begin_rendering",
            "cmd_bind_scene_solid_quad_pipeline",
            "cmd_bind_scene_vertex_buffer",
            "cmd_bind_scene_index_buffer",
            "cmd_draw_indexed_per_quad",
            "cmd_end_rendering",
            "cmd_pipeline_barrier2_present",
            "queue_submit2_present",
            "queue_present_khr",
        ]
    } else if sampled_image_pending {
        &[
            "cmd_pipeline_barrier2_swapchain_attachment",
            "cmd_begin_rendering",
            "cmd_bind_scene_sampled_image_pipeline",
            "cmd_bind_sampled_image_vertex_buffer",
            "cmd_bind_sampled_image_index_buffer",
            "cmd_bind_scene_descriptor_heap",
            "cmd_draw_indexed_per_image_quad",
            "cmd_end_rendering",
            "cmd_pipeline_barrier2_present",
            "queue_submit2_present",
            "queue_present_khr",
        ]
    } else if fast_clear_color_ready {
        &["delegate_to_vulkanalia_clear_present"]
    } else {
        &["wait_for_scene_recordable_draw_ops"]
    }
}

fn saturating_u32(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
}

fn native_vulkan_vulkanalia_scene_create_shader_module(
    device: &Device,
    code: &[u32],
    label: &'static str,
) -> Result<vk::ShaderModule, String> {
    if code.first().copied() != Some(0x0723_0203) {
        return Err(format!("{label} shader is not valid SPIR-V bytecode"));
    }
    let create_info = vk::ShaderModuleCreateInfo::builder()
        .code(code)
        .code_size(native_vulkan_vulkanalia_scene_shader_code_size_bytes(code));
    unsafe { device.create_shader_module(&create_info, None) }
        .map_err(|err| format!("vkCreateShaderModule(vulkanalia {label}): {err:?}"))
}

fn native_vulkan_vulkanalia_scene_shader_code_size_bytes(code: &[u32]) -> usize {
    std::mem::size_of_val(code)
}

fn native_vulkan_vulkanalia_scene_color_subresource_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .base_mip_level(0)
        .level_count(1)
        .base_array_layer(0)
        .layer_count(1)
        .build()
}

const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SOLID_QUAD_VERTEX_SPIRV: [u32; 379] = [
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

const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SOLID_QUAD_FRAGMENT_SPIRV: [u32; 94] = [
    119734787, 65536, 524299, 13, 0, 131089, 1, 393227, 1, 1280527431, 1685353262, 808793134, 0,
    196622, 0, 1, 458767, 4, 4, 1852399981, 0, 9, 11, 196624, 4, 7, 196611, 2, 450, 262149, 4,
    1852399981, 0, 327685, 9, 1601467759, 1869377379, 114, 262149, 11, 1868783478, 7499628, 262215,
    9, 30, 0, 262215, 11, 30, 0, 131091, 2, 196641, 3, 2, 196630, 6, 32, 262167, 7, 6, 4, 262176,
    8, 3, 7, 262203, 8, 9, 3, 262176, 10, 1, 7, 262203, 10, 11, 1, 327734, 2, 4, 0, 3, 131320, 5,
    262205, 7, 12, 11, 196670, 9, 12, 65789, 65592,
];

const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_VERTEX_SPIRV: [u32; 446] = [
    119734787, 65536, 851979, 62, 0, 131089, 1, 393227, 1, 1280527431, 1685353262, 808793134, 0,
    196622, 0, 1, 720911, 0, 4, 1852399981, 0, 11, 43, 55, 56, 59, 60, 196611, 2, 450, 655364,
    1197427783, 1279741775, 1885560645, 1953718128, 1600482425, 1701734764, 1919509599, 1769235301,
    25974, 524292, 1197427783, 1279741775, 1852399429, 1685417059, 1768185701, 1952671090, 6649449,
    262149, 4, 1852399981, 0, 196613, 9, 6513774, 327685, 11, 1885302377, 1953067887, 7237481,
    262149, 17, 1752397136, 0, 327686, 17, 0, 1702131813, 29806, 196613, 19, 25456, 393221, 41,
    1348430951, 1700164197, 2019914866, 0, 393222, 41, 0, 1348430951, 1953067887, 7237481, 458758,
    41, 1, 1348430951, 1953393007, 1702521171, 0, 458758, 41, 2, 1130327143, 1148217708,
    1635021673, 6644590, 458758, 41, 3, 1130327143, 1147956341, 1635021673, 6644590, 196613, 43, 0,
    262149, 55, 1601467759, 30325, 262149, 56, 1969188457, 118, 327685, 59, 1601467759, 1667330159,
    7959657, 327685, 60, 1868525161, 1768120688, 31092, 262215, 11, 30, 0, 196679, 17, 2, 327752,
    17, 0, 35, 0, 196679, 41, 2, 327752, 41, 0, 11, 0, 327752, 41, 1, 11, 1, 327752, 41, 2, 11, 3,
    327752, 41, 3, 11, 4, 262215, 55, 30, 0, 262215, 56, 30, 1, 262215, 59, 30, 1, 262215, 60, 30,
    2, 131091, 2, 196641, 3, 2, 196630, 6, 32, 262167, 7, 6, 2, 262176, 8, 7, 7, 262176, 10, 1, 7,
    262203, 10, 11, 1, 262165, 12, 32, 0, 262187, 12, 13, 0, 262176, 14, 1, 6, 196638, 17, 7,
    262176, 18, 9, 17, 262203, 18, 19, 9, 262165, 20, 32, 1, 262187, 20, 21, 0, 262176, 22, 9, 6,
    262187, 6, 26, 1073741824, 262187, 6, 28, 1065353216, 262187, 12, 30, 1, 262167, 39, 6, 4,
    262172, 40, 6, 30, 393246, 41, 39, 6, 40, 40, 262176, 42, 3, 41, 262203, 42, 43, 3, 262176, 44,
    7, 6, 262187, 6, 50, 0, 262176, 52, 3, 39, 262176, 54, 3, 7, 262203, 54, 55, 3, 262203, 10, 56,
    1, 262176, 58, 3, 6, 262203, 58, 59, 3, 262203, 14, 60, 1, 327734, 2, 4, 0, 3, 131320, 5,
    262203, 8, 9, 7, 327745, 14, 15, 11, 13, 262205, 6, 16, 15, 393281, 22, 23, 19, 21, 13, 262205,
    6, 24, 23, 327816, 6, 25, 16, 24, 327813, 6, 27, 25, 26, 327811, 6, 29, 27, 28, 327745, 14, 31,
    11, 30, 262205, 6, 32, 31, 393281, 22, 33, 19, 21, 30, 262205, 6, 34, 33, 327816, 6, 35, 32,
    34, 327813, 6, 36, 35, 26, 327811, 6, 37, 36, 28, 327760, 7, 38, 29, 37, 196670, 9, 38, 327745,
    44, 45, 9, 13, 262205, 6, 46, 45, 327745, 44, 47, 9, 30, 262205, 6, 48, 47, 262271, 6, 49, 48,
    458832, 39, 51, 46, 49, 50, 28, 327745, 52, 53, 43, 21, 196670, 53, 51, 262205, 7, 57, 56,
    196670, 55, 57, 262205, 6, 61, 60, 196670, 59, 61, 65789, 65592,
];

const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_FRAGMENT_SPIRV: [u32; 259] = [
    119734787, 65536, 851979, 38, 0, 131089, 1, 393227, 1, 1280527431, 1685353262, 808793134, 0,
    196622, 0, 1, 524303, 4, 4, 1852399981, 0, 17, 21, 31, 196624, 4, 7, 196611, 2, 450, 655364,
    1197427783, 1279741775, 1885560645, 1953718128, 1600482425, 1701734764, 1919509599, 1769235301,
    25974, 524292, 1197427783, 1279741775, 1852399429, 1685417059, 1768185701, 1952671090, 6649449,
    262149, 4, 1852399981, 0, 262149, 9, 1869377379, 114, 327685, 13, 1852138355, 1835622245,
    6645601, 262149, 17, 1969188457, 118, 327685, 21, 1601467759, 1869377379, 114, 327685, 31,
    1868525161, 1768120688, 31092, 262215, 13, 33, 0, 262215, 13, 34, 0, 262215, 17, 30, 0, 262215,
    21, 30, 0, 262215, 31, 30, 1, 131091, 2, 196641, 3, 2, 196630, 6, 32, 262167, 7, 6, 4, 262176,
    8, 7, 7, 589849, 10, 6, 1, 0, 0, 0, 1, 0, 196635, 11, 10, 262176, 12, 0, 11, 262203, 12, 13, 0,
    262167, 15, 6, 2, 262176, 16, 1, 15, 262203, 16, 17, 1, 262176, 20, 3, 7, 262203, 20, 21, 3,
    262167, 22, 6, 3, 262165, 25, 32, 0, 262187, 25, 26, 3, 262176, 27, 7, 6, 262176, 30, 1, 6,
    262203, 30, 31, 1, 327734, 2, 4, 0, 3, 131320, 5, 262203, 8, 9, 7, 262205, 11, 14, 13, 262205,
    15, 18, 17, 327767, 7, 19, 14, 18, 196670, 9, 19, 262205, 7, 23, 9, 524367, 22, 24, 23, 23, 0,
    1, 2, 327745, 27, 28, 9, 26, 262205, 6, 29, 28, 262205, 6, 32, 31, 327813, 6, 33, 29, 32,
    327761, 6, 34, 24, 0, 327761, 6, 35, 24, 1, 327761, 6, 36, 24, 2, 458832, 7, 37, 34, 35, 36,
    33, 196670, 21, 37, 65789, 65592,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> NativeVulkanVulkanaliaSceneDrawPassInput {
        NativeVulkanVulkanaliaSceneDrawPassInput {
            plan_ready: true,
            native_draw_ready: true,
            draw_op_count: 1,
            backend_status: "solid-quad-recording-ready",
            blocking_reason: None,
            fast_clear_color_ready: false,
            clear_background_op_count: 0,
            quad_recording_ready: true,
            quad_recording_step_count: 1,
            quad_vertex_buffer_bytes: 96,
            quad_index_buffer_bytes: 24,
            sampled_image_recording_ready: false,
            sampled_image_implicit_full_extent_ready: false,
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
    fn solid_quad_scene_path_is_dynamic_rendering_recordable() {
        let snapshot = native_vulkan_vulkanalia_scene_draw_pass_snapshot(input());

        assert!(snapshot.backend_ready);
        assert_eq!(
            snapshot.backend_status,
            "solid-quad-dynamic-rendering-recording-ready"
        );
        assert_eq!(
            snapshot.pipeline_labels,
            vec!["scene-solid-quad-alpha-blend"]
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
    fn sampled_image_scene_path_is_dynamic_rendering_recordable() {
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

        let snapshot = native_vulkan_vulkanalia_scene_draw_pass_snapshot(input);

        assert!(snapshot.backend_ready);
        assert_eq!(
            snapshot.backend_status,
            "sampled-image-dynamic-rendering-recording-ready"
        );
        assert_eq!(snapshot.blocking_reason, None);
        assert_eq!(
            snapshot.pipeline_labels,
            vec!["scene-sampled-image-alpha-blend"]
        );
        assert_eq!(snapshot.descriptor_set_count, 0);
        assert_eq!(
            snapshot.vertex_stride_bytes,
            SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES
        );
        assert_eq!(snapshot.draw_indexed_count, 1);
        assert!(
            snapshot
                .command_order
                .contains(&"cmd_bind_scene_descriptor_heap")
        );
        assert!(
            snapshot
                .command_order
                .contains(&"cmd_draw_indexed_per_image_quad")
        );
    }

    #[test]
    fn sampled_image_implicit_full_extent_path_is_present_ready() {
        let mut input = input();
        input.draw_op_count = 1;
        input.backend_status = "sampled-image-implicit-full-extent-ready";
        input.quad_recording_ready = false;
        input.quad_recording_step_count = 0;
        input.quad_vertex_buffer_bytes = 0;
        input.quad_index_buffer_bytes = 0;
        input.sampled_image_implicit_full_extent_ready = true;
        input.sampled_image_op_count = 1;
        input.vector_shape_op_count = 0;

        let snapshot = native_vulkan_vulkanalia_scene_draw_pass_snapshot(input);

        assert!(snapshot.backend_ready);
        assert_eq!(
            snapshot.backend_status,
            "sampled-image-implicit-full-extent-present-ready"
        );
        assert_eq!(snapshot.blocking_reason, None);
        assert_eq!(snapshot.sampled_image_quad_count, 1);
        assert_eq!(
            snapshot.pipeline_labels,
            vec!["scene-sampled-image-alpha-blend"]
        );
        assert_eq!(
            snapshot.vertex_stride_bytes,
            SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES
        );
        assert_eq!(snapshot.draw_indexed_count, 1);
        assert!(snapshot.uses_dynamic_rendering);
        assert!(snapshot.uses_synchronization2);
        assert!(snapshot.uses_submit2);
        assert!(
            snapshot
                .command_order
                .contains(&"cmd_draw_indexed_per_image_quad")
        );
    }

    #[test]
    fn mixed_full_extent_sampled_image_path_is_present_ready() {
        let mut input = input();
        input.draw_op_count = 2;
        input.backend_status = "mixed-quad-sampled-image-implicit-full-extent-ready";
        input.sampled_image_implicit_full_extent_ready = true;
        input.sampled_image_op_count = 1;

        let snapshot = native_vulkan_vulkanalia_scene_draw_pass_snapshot(input);

        assert!(snapshot.backend_ready);
        assert_eq!(
            snapshot.backend_status,
            "mixed-quad-sampled-image-implicit-full-extent-present-ready"
        );
        assert_eq!(snapshot.blocking_reason, None);
        assert_eq!(snapshot.solid_quad_count, 1);
        assert_eq!(snapshot.sampled_image_quad_count, 1);
        assert_eq!(
            snapshot.pipeline_labels,
            vec![
                "scene-solid-quad-alpha-blend",
                "scene-sampled-image-alpha-blend"
            ]
        );
        assert_eq!(snapshot.draw_indexed_count, 2);
        assert!(snapshot.uses_dynamic_rendering);
        assert!(snapshot.uses_synchronization2);
        assert!(snapshot.uses_submit2);
        assert!(
            snapshot
                .command_order
                .contains(&"cmd_draw_indexed_in_scene_layer_order")
        );
    }

    #[test]
    fn solid_quad_pipeline_template_uses_dynamic_rendering_and_push_constants() {
        let snapshot = native_vulkan_vulkanalia_scene_solid_quad_pipeline_snapshot(
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
    fn sampled_image_pipeline_template_uses_descriptor_heap_and_dynamic_rendering() {
        let snapshot = native_vulkan_vulkanalia_scene_sampled_image_pipeline_snapshot(
            vk::Format::B8G8R8A8_SRGB,
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
        );

        assert_eq!(snapshot.target_format, "B8G8R8A8_SRGB");
        assert_eq!(snapshot.extent, (3840, 2160));
        assert!(!snapshot.descriptor_set_layout_created);
        assert_eq!(snapshot.descriptor_type, "combined-image-sampler");
        assert_eq!(snapshot.descriptor_binding, 0);
        assert_eq!(snapshot.vertex_input_attribute_count, 3);
        assert_eq!(
            snapshot.vertex_stride_bytes,
            SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES
        );
        assert_eq!(snapshot.vertex_uv_format, "R32G32_SFLOAT");
        assert_eq!(snapshot.vertex_opacity_format, "R32_SFLOAT");
        assert_eq!(
            snapshot.sampled_image_model,
            "retained BC7_UNORM_BLOCK sampled image -> VK_EXT_descriptor_heap constant-offset mapping -> fragment shader"
        );
        assert!(snapshot.uses_pipeline_rendering_create_info);
        assert!(snapshot.uses_dynamic_rendering);
        assert!(snapshot.uses_synchronization2);
        assert!(snapshot.uses_submit2);
        assert_eq!(snapshot.descriptor_set_count, 0);
        assert_eq!(snapshot.descriptor_model, "VK_EXT_descriptor_heap");
        assert!(snapshot.descriptor_heap_mapping_enabled);
        assert!(snapshot.descriptor_heap_pipeline_flag_enabled);
        assert!(snapshot.descriptor_set_layout_create_flags.is_empty());
        assert!(!snapshot.uses_push_descriptor_fast_path);
    }

    #[test]
    fn sampled_image_pipeline_template_can_use_descriptor_heap_mapping() {
        let snapshot = native_vulkan_vulkanalia_scene_sampled_image_pipeline_snapshot(
            vk::Format::B8G8R8A8_SRGB,
            vk::Extent2D {
                width: 3840,
                height: 2160,
            },
        );

        assert_eq!(snapshot.descriptor_set_count, 0);
        assert_eq!(snapshot.descriptor_model, "VK_EXT_descriptor_heap");
        assert!(snapshot.descriptor_heap_mapping_enabled);
        assert!(snapshot.descriptor_heap_pipeline_flag_enabled);
        assert!(
            snapshot
                .sampled_image_model
                .contains("VK_EXT_descriptor_heap")
        );
        assert!(!snapshot.uses_push_descriptor_fast_path);
    }

    #[test]
    fn mixed_ordered_draw_steps_follow_scene_layer_order() {
        let solid_commands = [VulkanaliaSceneSolidQuadDrawCommand {
            layer_index: 2,
            first_index: 0,
            index_count: 6,
        }];
        let sampled_commands = [
            VulkanaliaSceneSampledImageDrawCommand {
                layer_index: 1,
                descriptor_binding: VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap {
                    resource_index: 0,
                },
                first_index: 0,
                index_count: 6,
            },
            VulkanaliaSceneSampledImageDrawCommand {
                layer_index: 3,
                descriptor_binding: VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap {
                    resource_index: 1,
                },
                first_index: 6,
                index_count: 6,
            },
        ];

        let ordered =
            native_vulkan_vulkanalia_scene_ordered_draw_steps(&solid_commands, &sampled_commands);
        let order = ordered
            .iter()
            .map(|step| (step.layer_index, step.pipeline.label(), step.command_index))
            .collect::<Vec<_>>();

        assert_eq!(
            order,
            vec![
                (1, "sampled-image", 0),
                (2, "solid-quad", 0),
                (3, "sampled-image", 1)
            ]
        );
    }

    #[test]
    fn solid_quad_shader_bytecode_is_inline_spirv() {
        assert_eq!(
            NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SOLID_QUAD_VERTEX_SPIRV[0],
            0x0723_0203
        );
        assert_eq!(
            NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SOLID_QUAD_FRAGMENT_SPIRV[0],
            0x0723_0203
        );
        assert_eq!(
            NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_VERTEX_SPIRV[0],
            0x0723_0203
        );
        assert_eq!(
            NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_FRAGMENT_SPIRV[0],
            0x0723_0203
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SOLID_QUAD_VERTEX_SPIRV
            ),
            1516
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SOLID_QUAD_FRAGMENT_SPIRV
            ),
            376
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_VERTEX_SPIRV
            ),
            1784
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_FRAGMENT_SPIRV
            ),
            1036
        );
    }

    #[test]
    fn solid_quad_command_order_records_dynamic_rendering_draw_indexed() {
        let order =
            native_vulkan_vulkanalia_scene_draw_pass_command_order(true, false, false, false);

        assert_eq!(order[0], "cmd_pipeline_barrier2_swapchain_attachment");
        assert!(order.contains(&"cmd_begin_rendering"));
        assert!(order.contains(&"cmd_bind_scene_solid_quad_pipeline"));
        assert!(order.contains(&"cmd_bind_scene_vertex_buffer"));
        assert!(order.contains(&"cmd_bind_scene_index_buffer"));
        assert!(order.contains(&"cmd_draw_indexed_per_quad"));
        assert!(order.contains(&"queue_submit2_present"));
        assert!(order.contains(&"queue_present_khr"));
    }

    #[test]
    fn mixed_scene_command_order_records_layer_ordered_draws() {
        let order =
            native_vulkan_vulkanalia_scene_draw_pass_command_order(false, true, false, true);

        assert!(order.contains(&"cmd_bind_scene_solid_quad_pipeline_as_needed"));
        assert!(order.contains(&"cmd_bind_scene_sampled_image_pipeline_as_needed"));
        assert!(order.contains(&"cmd_bind_scene_descriptor_heap_when_needed"));
        assert!(order.contains(&"cmd_draw_indexed_in_scene_layer_order"));
        assert!(order.contains(&"queue_submit2_present"));
    }

    #[test]
    fn mixed_scene_command_order_can_use_descriptor_heap() {
        let order =
            native_vulkan_vulkanalia_scene_draw_pass_command_order(false, true, false, true);

        assert!(order.contains(&"cmd_bind_scene_descriptor_heap_when_needed"));
        assert!(order.contains(&"cmd_draw_indexed_in_scene_layer_order"));
    }
}
