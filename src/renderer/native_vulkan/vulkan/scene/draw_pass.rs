#![allow(dead_code)]

use std::sync::atomic::AtomicUsize;

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, ExtDescriptorHeapExtensionDeviceCommands, HasBuilder};

use crate::core::SceneBlendMode;
use crate::renderer::SceneRenderAlphaTextureMode;
use crate::renderer::native_vulkan::effect_debug::{
    native_vulkan_effect_debug_enabled, native_vulkan_effect_debug_log_limited,
};

use super::descriptor_heap::{
    NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    VulkanaliaDescriptorHeapImageSamplerResources,
    native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_bind_info_for_image,
    native_vulkan_vulkanalia_descriptor_heap_sampler_bind_info_for_image,
};
use super::scene_sampled_image::VulkanaliaSceneSampledImageResources;

mod blend;

use self::blend::{
    native_vulkan_vulkanalia_scene_blend_mode_label,
    native_vulkan_vulkanalia_scene_color_attachment,
    native_vulkan_vulkanalia_scene_fragment_module_for_blend,
    native_vulkan_vulkanalia_scene_sampled_image_pipeline,
    native_vulkan_vulkanalia_scene_solid_quad_pipeline,
};

const SCENE_FULL_SOLID_QUAD_VERTEX_STRIDE_BYTES: u32 = 24;
const SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES: u32 = 44;
const SCENE_FULL_SOLID_QUAD_PUSH_CONSTANT_BYTES: u32 = 8;
const SCENE_FULL_SAMPLED_IMAGE_PUSH_CONSTANT_BYTES: u32 = 20;
pub(in crate::renderer::native_vulkan::vulkan) const SCENE_SAMPLED_IMAGE_TEXTURE_SLOT_BINDING_COUNT:
    usize = 8;
const SCENE_SAMPLED_IMAGE_ALPHA_TEXTURE_SLOT_DISABLED: u32 = u32::MAX;
static SCENE_DRAW_PASS_EFFECT_DEBUG_LOG_COUNT: AtomicUsize = AtomicUsize::new(0);

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
    pub vertex_effect_uv_format: &'static str,
    pub vertex_opacity_format: &'static str,
    pub vertex_tint_format: &'static str,
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
    pub(in crate::renderer::native_vulkan::vulkan) alpha_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) additive_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) multiply_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) screen_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) max_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) snapshot:
        NativeVulkanVulkanaliaSceneSolidQuadPipelineSnapshot,
}

pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneSampledImagePipelineResources {
    pub(in crate::renderer::native_vulkan::vulkan) pipeline_layout: vk::PipelineLayout,
    pub(in crate::renderer::native_vulkan::vulkan) alpha_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) additive_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) multiply_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) screen_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) max_pipeline: vk::Pipeline,
    pub(in crate::renderer::native_vulkan::vulkan) snapshot:
        NativeVulkanVulkanaliaSceneSampledImagePipelineSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::vulkan) enum VulkanaliaSceneSampledImageDescriptorBinding {
    DescriptorHeap {
        descriptor_group_base_index: u32,
        texture_slot_bindings:
            Vec<super::present::NativeVulkanVulkanaliaSceneTextureSlotResourceBinding>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::vulkan) enum VulkanaliaSceneSampledImageRenderTarget {
    Swapchain,
    EffectTarget { target_index: u32, clear: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneSampledImageDrawCommand {
    pub(in crate::renderer::native_vulkan::vulkan) layer_index: usize,
    pub(in crate::renderer::native_vulkan::vulkan) last_layer_index: usize,
    pub(in crate::renderer::native_vulkan::vulkan) material:
        super::present::NativeVulkanVulkanaliaSceneSampledImageMaterial,
    pub(in crate::renderer::native_vulkan::vulkan) descriptor_binding:
        VulkanaliaSceneSampledImageDescriptorBinding,
    pub(in crate::renderer::native_vulkan::vulkan) render_target:
        VulkanaliaSceneSampledImageRenderTarget,
    pub(in crate::renderer::native_vulkan::vulkan) first_index: u32,
    pub(in crate::renderer::native_vulkan::vulkan) index_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::vulkan) struct VulkanaliaSceneSolidQuadDrawCommand {
    pub(in crate::renderer::native_vulkan::vulkan) layer_index: usize,
    pub(in crate::renderer::native_vulkan::vulkan) last_layer_index: usize,
    pub(in crate::renderer::native_vulkan::vulkan) blend:
        super::present::NativeVulkanVulkanaliaSceneBlendState,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VulkanaliaSceneBoundDrawPipeline {
    SolidQuad(super::present::NativeVulkanVulkanaliaSceneBlendState),
    SampledImage(super::present::NativeVulkanVulkanaliaSceneBlendState),
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

fn native_vulkan_vulkanalia_scene_bound_pipeline_key(
    draw: &VulkanaliaSceneOrderedDrawStep,
    solid_commands: &[VulkanaliaSceneSolidQuadDrawCommand],
    sampled_commands: &[VulkanaliaSceneSampledImageDrawCommand],
) -> VulkanaliaSceneBoundDrawPipeline {
    match draw.pipeline {
        VulkanaliaSceneOrderedDrawPipeline::SolidQuad => {
            VulkanaliaSceneBoundDrawPipeline::SolidQuad(solid_commands[draw.command_index].blend)
        }
        VulkanaliaSceneOrderedDrawPipeline::SampledImage => {
            VulkanaliaSceneBoundDrawPipeline::SampledImage(
                sampled_commands[draw.command_index]
                    .material
                    .render_state
                    .blend,
            )
        }
    }
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
        && input.sampled_image_recording_step_count <= input.sampled_image_op_count
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
        && input.sampled_image_recording_step_count <= input.sampled_image_op_count
        && input
            .quad_recording_step_count
            .saturating_add(input.sampled_image_op_count)
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
        vec![
            "scene-solid-quad-alpha-blend",
            "scene-solid-quad-additive-blend",
            "scene-solid-quad-multiply-blend",
            "scene-solid-quad-screen-blend",
            "scene-solid-quad-max-blend",
        ]
    } else if mixed_quad_sampled_image_ready || mixed_quad_sampled_image_implicit_full_extent_ready
    {
        vec![
            "scene-solid-quad-alpha-blend",
            "scene-solid-quad-additive-blend",
            "scene-solid-quad-multiply-blend",
            "scene-solid-quad-screen-blend",
            "scene-solid-quad-max-blend",
            "scene-sampled-image-alpha-blend",
            "scene-sampled-image-additive-blend",
            "scene-sampled-image-multiply-blend",
            "scene-sampled-image-screen-blend",
            "scene-sampled-image-max-blend",
        ]
    } else if sampled_image_pending || sampled_image_implicit_full_extent_ready {
        vec![
            "scene-sampled-image-alpha-blend",
            "scene-sampled-image-additive-blend",
            "scene-sampled-image-multiply-blend",
            "scene-sampled-image-screen-blend",
            "scene-sampled-image-max-blend",
        ]
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
                let premultiplied_fragment_module =
                native_vulkan_vulkanalia_scene_create_shader_module(
                    device,
                    &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SOLID_QUAD_PREMULTIPLIED_FRAGMENT_SPIRV,
                    "scene solid quad premultiplied fragment",
                )?;
                let result = native_vulkan_vulkanalia_create_scene_solid_quad_blend_pipelines(
                    device,
                    target_format,
                    extent,
                    pipeline_layout,
                    vertex_module,
                    fragment_module,
                    premultiplied_fragment_module,
                );
                unsafe {
                    device.destroy_shader_module(premultiplied_fragment_module, None);
                }
                result
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

#[allow(clippy::too_many_arguments)]
fn native_vulkan_vulkanalia_create_scene_solid_quad_blend_pipelines(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
    pipeline_layout: vk::PipelineLayout,
    vertex_module: vk::ShaderModule,
    fragment_module: vk::ShaderModule,
    premultiplied_fragment_module: vk::ShaderModule,
) -> Result<VulkanaliaSceneSolidQuadPipelineResources, String> {
    let shader_entry = b"main\0";
    let create_pipeline = |blend_mode| -> Result<vk::Pipeline, String> {
        let selected_fragment_module = native_vulkan_vulkanalia_scene_fragment_module_for_blend(
            blend_mode,
            fragment_module,
            premultiplied_fragment_module,
        );
        let stages = [
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vertex_module)
                .name(shader_entry)
                .build(),
            vk::PipelineShaderStageCreateInfo::builder()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(selected_fragment_module)
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
        let color_attachment = native_vulkan_vulkanalia_scene_color_attachment(blend_mode);
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
            device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
        }
        .map_err(|err| {
            format!(
                "vkCreateGraphicsPipelines(vulkanalia scene quad {}): {err:?}",
                native_vulkan_vulkanalia_scene_blend_mode_label(blend_mode)
            )
        })?;
        Ok(pipelines[0])
    };

    let mut created_pipelines = Vec::with_capacity(5);
    let mut create_tracked_pipeline = |blend_mode| -> Result<vk::Pipeline, String> {
        let pipeline = create_pipeline(blend_mode)?;
        created_pipelines.push(pipeline);
        Ok(pipeline)
    };
    let result = (|| -> Result<VulkanaliaSceneSolidQuadPipelineResources, String> {
        let alpha_pipeline = create_tracked_pipeline(SceneBlendMode::Alpha)?;
        let additive_pipeline = create_tracked_pipeline(SceneBlendMode::Additive)?;
        let multiply_pipeline = create_tracked_pipeline(SceneBlendMode::Multiply)?;
        let screen_pipeline = create_tracked_pipeline(SceneBlendMode::Screen)?;
        let max_pipeline = create_tracked_pipeline(SceneBlendMode::Max)?;
        Ok(VulkanaliaSceneSolidQuadPipelineResources {
            pipeline_layout,
            alpha_pipeline,
            additive_pipeline,
            multiply_pipeline,
            screen_pipeline,
            max_pipeline,
            snapshot: native_vulkan_vulkanalia_scene_solid_quad_pipeline_snapshot(
                target_format,
                extent,
            ),
        })
    })();
    if result.is_err() {
        unsafe {
            for pipeline in created_pipelines {
                device.destroy_pipeline(pipeline, None);
            }
        }
    }
    result
}

pub(in crate::renderer::native_vulkan::vulkan) fn native_vulkan_vulkanalia_destroy_scene_solid_quad_pipeline_resources(
    device: &Device,
    resources: VulkanaliaSceneSolidQuadPipelineResources,
) {
    unsafe {
        device.destroy_pipeline(resources.alpha_pipeline, None);
        device.destroy_pipeline(resources.additive_pipeline, None);
        device.destroy_pipeline(resources.multiply_pipeline, None);
        device.destroy_pipeline(resources.screen_pipeline, None);
        device.destroy_pipeline(resources.max_pipeline, None);
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
        blend_model: "solid rgba with opacity; alpha/additive/multiply/screen/max blend pipeline selected per draw command",
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
            .stage_flags(vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT)
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
                let premultiplied_fragment_module =
                    native_vulkan_vulkanalia_scene_create_shader_module(
                        device,
                        &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_PREMULTIPLIED_FRAGMENT_SPIRV,
                        "scene sampled image premultiplied fragment",
                    )?;
                let result =
                    (|| -> Result<VulkanaliaSceneSampledImagePipelineResources, String> {
                        let create_pipeline = |blend_mode| {
                            native_vulkan_vulkanalia_create_scene_sampled_image_pipeline(
                                device,
                                target_format,
                                extent,
                                descriptor_heap_plan,
                                pipeline_layout,
                                vertex_module,
                                native_vulkan_vulkanalia_scene_fragment_module_for_blend(
                                    blend_mode,
                                    fragment_module,
                                    premultiplied_fragment_module,
                                ),
                                blend_mode,
                            )
                        };
                        let alpha_pipeline = create_pipeline(SceneBlendMode::Alpha)?;
                        let result =
                            (|| -> Result<VulkanaliaSceneSampledImagePipelineResources, String> {
                                let additive_pipeline = create_pipeline(SceneBlendMode::Additive)?;
                                let result = (|| -> Result<
                                    VulkanaliaSceneSampledImagePipelineResources,
                                    String,
                                > {
                                    let multiply_pipeline =
                                        create_pipeline(SceneBlendMode::Multiply)?;
                                    let result = (|| -> Result<
                                        VulkanaliaSceneSampledImagePipelineResources,
                                        String,
                                    > {
                                        let screen_pipeline =
                                            create_pipeline(SceneBlendMode::Screen)?;
                                        let result = (|| -> Result<
                                            VulkanaliaSceneSampledImagePipelineResources,
                                            String,
                                        > {
                                            let max_pipeline =
                                                create_pipeline(SceneBlendMode::Max)?;
                                            Ok(VulkanaliaSceneSampledImagePipelineResources {
                                                pipeline_layout,
                                                alpha_pipeline,
                                                additive_pipeline,
                                                multiply_pipeline,
                                                screen_pipeline,
                                                max_pipeline,
                                                snapshot:
                                                    native_vulkan_vulkanalia_scene_sampled_image_pipeline_snapshot(
                                                        target_format,
                                                        extent,
                                                    ),
                                            })
                                        })();
                                        if result.is_err() {
                                            unsafe {
                                                device.destroy_pipeline(screen_pipeline, None);
                                            }
                                        }
                                        result
                                    })();
                                    if result.is_err() {
                                        unsafe {
                                            device.destroy_pipeline(multiply_pipeline, None);
                                        }
                                    }
                                    result
                                })();
                                if result.is_err() {
                                    unsafe {
                                        device.destroy_pipeline(additive_pipeline, None);
                                    }
                                }
                                result
                            })();
                        if result.is_err() {
                            unsafe {
                                device.destroy_pipeline(alpha_pipeline, None);
                            }
                        }
                        result
                    })();
                unsafe {
                    device.destroy_shader_module(premultiplied_fragment_module, None);
                }
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
        device.destroy_pipeline(resources.alpha_pipeline, None);
        device.destroy_pipeline(resources.additive_pipeline, None);
        device.destroy_pipeline(resources.multiply_pipeline, None);
        device.destroy_pipeline(resources.screen_pipeline, None);
        device.destroy_pipeline(resources.max_pipeline, None);
        device.destroy_pipeline_layout(resources.pipeline_layout, None);
    }
}

#[allow(clippy::too_many_arguments)]
fn native_vulkan_vulkanalia_create_scene_sampled_image_pipeline(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    pipeline_layout: vk::PipelineLayout,
    vertex_module: vk::ShaderModule,
    fragment_module: vk::ShaderModule,
    blend_mode: SceneBlendMode,
) -> Result<vk::Pipeline, String> {
    let shader_entry = b"main\0";
    if descriptor_heap_plan.image_count < SCENE_SAMPLED_IMAGE_TEXTURE_SLOT_BINDING_COUNT {
        return Err(format!(
            "scene sampled-image pipeline requires at least {} descriptor heap texture slots, got {}",
            SCENE_SAMPLED_IMAGE_TEXTURE_SLOT_BINDING_COUNT, descriptor_heap_plan.image_count
        ));
    }
    let descriptor_heap_mappings = (0..SCENE_SAMPLED_IMAGE_TEXTURE_SLOT_BINDING_COUNT)
        .map(|binding| {
            native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_binding_mapping(
                descriptor_heap_plan,
                binding as u32,
                binding,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut descriptor_heap_mapping_info =
        vk::ShaderDescriptorSetAndBindingMappingInfoEXT::builder()
            .mappings(&descriptor_heap_mappings)
            .build();
    let mut fragment_stage = vk::PipelineShaderStageCreateInfo::builder()
        .stage(vk::ShaderStageFlags::FRAGMENT)
        .module(fragment_module)
        .name(shader_entry);
    fragment_stage = fragment_stage.push_next(&mut descriptor_heap_mapping_info);
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
            .format(vk::Format::R32G32_SFLOAT)
            .offset(16)
            .build(),
        vk::VertexInputAttributeDescription::builder()
            .location(3)
            .binding(0)
            .format(vk::Format::R32_SFLOAT)
            .offset(24)
            .build(),
        vk::VertexInputAttributeDescription::builder()
            .location(4)
            .binding(0)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(28)
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
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let dynamic_state = vk::PipelineDynamicStateCreateInfo::builder()
        .dynamic_states(&dynamic_states)
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
    let color_attachment = native_vulkan_vulkanalia_scene_color_attachment(blend_mode);
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
        .dynamic_state(&dynamic_state)
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
        device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
    }
    .map_err(|err| {
        format!(
            "vkCreateGraphicsPipelines(vulkanalia scene sampled image {}): {err:?}",
            native_vulkan_vulkanalia_scene_blend_mode_label(blend_mode)
        )
    })?;
    Ok(pipelines[0])
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
        vertex_input_attribute_count: 5,
        vertex_stride_bytes: SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES,
        vertex_position_format: "R32G32_SFLOAT",
        vertex_uv_format: "R32G32_SFLOAT",
        vertex_effect_uv_format: "R32G32_SFLOAT",
        vertex_opacity_format: "R32_SFLOAT",
        vertex_tint_format: "R32G32B32A32_SFLOAT",
        descriptor_set_count: 0,
        descriptor_model: "VK_EXT_descriptor_heap",
        descriptor_heap_mapping_enabled: true,
        descriptor_heap_pipeline_flag_enabled: true,
        descriptor_set_layout_create_flags: Vec::new(),
        descriptor_type: "combined-image-sampler",
        descriptor_binding: 0,
        push_constant_bytes: SCENE_FULL_SAMPLED_IMAGE_PUSH_CONSTANT_BYTES,
        push_constant_model: "scene-space pixel extent -> NDC conversion in vertex shader",
        blend_model: "sampled rgba with opacity; alpha/additive/multiply/screen/max blend pipeline selected per draw command",
        sampled_image_model: "retained native sampled image -> VK_EXT_descriptor_heap constant-offset mapping -> fragment shader",
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
            .flags(vk::CommandBufferUsageFlags::empty())
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
            pipeline_resources.alpha_pipeline,
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
        let mut bound_pipeline = None;
        for solid_draw in solid_quad_draw.draw_commands {
            if bound_pipeline != Some(solid_draw.blend) {
                device.cmd_bind_pipeline(
                    command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    native_vulkan_vulkanalia_scene_solid_quad_pipeline(
                        solid_quad_draw.pipeline_resources,
                        solid_draw.blend.mode,
                    ),
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
                bound_pipeline = Some(solid_draw.blend);
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
        if draw.render_target != VulkanaliaSceneSampledImageRenderTarget::Swapchain {
            return Err(
                "scene sampled-image inside-rendering helper only supports swapchain draw targets"
                    .to_owned(),
            );
        }
        match &draw.descriptor_binding {
            VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap {
                descriptor_group_base_index,
                texture_slot_bindings,
            } => {
                let Some(descriptor_heap_draw) = descriptor_heap_draw else {
                    return Err(
                        "scene sampled-image descriptor heap draw requires heap resources"
                            .to_owned(),
                    );
                };
                let descriptor_group_end = *descriptor_group_base_index as usize
                    + SCENE_SAMPLED_IMAGE_TEXTURE_SLOT_BINDING_COUNT;
                if descriptor_group_end > descriptor_heap_draw.resources.plan.image_count {
                    return Err(format!(
                        "scene sampled-image descriptor heap group {}..{} exceeds heap image count {}",
                        descriptor_group_base_index,
                        descriptor_group_end,
                        descriptor_heap_draw.resources.plan.image_count
                    ));
                }
                if texture_slot_bindings.is_empty()
                    || texture_slot_bindings.len() > SCENE_SAMPLED_IMAGE_TEXTURE_SLOT_BINDING_COUNT
                {
                    return Err(format!(
                        "scene sampled-image texture slot count {} exceeds descriptor binding count {}",
                        texture_slot_bindings.len(),
                        SCENE_SAMPLED_IMAGE_TEXTURE_SLOT_BINDING_COUNT
                    ));
                }
                if let Some(alpha_texture_slot) = draw.material.alpha_texture_slot
                    && !texture_slot_bindings
                        .iter()
                        .any(|binding| binding.slot == alpha_texture_slot)
                {
                    return Err(format!(
                        "scene sampled-image alpha texture slot {alpha_texture_slot} has no resource binding"
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
        set_scene_dynamic_viewport_and_scissor(device, command_buffer, extent);
        let solid_push_constants = [extent.width as f32, extent.height as f32];
        let solid_push_constant_bytes = std::slice::from_raw_parts(
            solid_push_constants.as_ptr().cast::<u8>(),
            SCENE_FULL_SOLID_QUAD_PUSH_CONSTANT_BYTES as usize,
        );
        let mut bound_pipeline: Option<VulkanaliaSceneBoundDrawPipeline> = None;
        let mut bound_descriptor_heap_group: Option<u32> = None;
        for draw in &ordered_draws {
            match draw.pipeline {
                VulkanaliaSceneOrderedDrawPipeline::SolidQuad => {
                    let solid_draw = &solid_draw_commands[draw.command_index];
                    let pipeline_key =
                        VulkanaliaSceneBoundDrawPipeline::SolidQuad(solid_draw.blend);
                    if bound_pipeline != Some(pipeline_key) {
                        let solid_resources = solid_quad_draw
                            .as_ref()
                            .expect("solid draw resources present");
                        device.cmd_bind_pipeline(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            native_vulkan_vulkanalia_scene_solid_quad_pipeline(
                                solid_resources.pipeline_resources,
                                solid_draw.blend.mode,
                            ),
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
                        bound_pipeline = Some(pipeline_key);
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
                    let pipeline_key = VulkanaliaSceneBoundDrawPipeline::SampledImage(
                        sampled_draw.material.render_state.blend,
                    );
                    if bound_pipeline != Some(pipeline_key) {
                        device.cmd_bind_pipeline(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            native_vulkan_vulkanalia_scene_sampled_image_pipeline(
                                pipeline_resources,
                                sampled_draw.material.render_state.blend.mode,
                            ),
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
                        bound_pipeline = Some(pipeline_key);
                    }
                    let VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap {
                        descriptor_group_base_index,
                        ..
                    } = &sampled_draw.descriptor_binding;
                    if bound_descriptor_heap_group != Some(*descriptor_group_base_index) {
                        let descriptor_heap_draw =
                            descriptor_heap_draw.expect("descriptor heap draw resources present");
                        bind_scene_sampled_image_descriptor_heap_for_descriptor_group(
                            device,
                            command_buffer,
                            descriptor_heap_draw,
                            *descriptor_group_base_index,
                        )?;
                        bound_descriptor_heap_group = Some(*descriptor_group_base_index);
                    }
                    push_scene_sampled_image_constants(
                        device,
                        command_buffer,
                        pipeline_resources.pipeline_layout,
                        extent,
                        sampled_draw.material.alpha_texture_slot,
                        sampled_draw.material.alpha_texture_mode,
                        0,
                    );
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

fn push_scene_sampled_image_constants(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    pipeline_layout: vk::PipelineLayout,
    extent: vk::Extent2D,
    alpha_texture_slot: Option<u32>,
    alpha_texture_mode: SceneRenderAlphaTextureMode,
    elapsed_ms: u64,
) {
    let time_seconds = (elapsed_ms as f32) * 0.001;
    if native_vulkan_effect_debug_enabled() && alpha_texture_slot.is_some() {
        native_vulkan_effect_debug_log_limited(
            &SCENE_DRAW_PASS_EFFECT_DEBUG_LOG_COUNT,
            48,
            "vulkan.push-constants",
            format_args!(
                "extent={}x{} alpha_slot={:?} mode={} shader_code={} time_seconds={:.3}",
                extent.width,
                extent.height,
                alpha_texture_slot,
                alpha_texture_mode.as_str(),
                alpha_texture_mode.shader_code(),
                time_seconds,
            ),
        );
    }
    let alpha_texture_slot =
        alpha_texture_slot.unwrap_or(SCENE_SAMPLED_IMAGE_ALPHA_TEXTURE_SLOT_DISABLED);
    let mut push_constant_bytes = [0u8; SCENE_FULL_SAMPLED_IMAGE_PUSH_CONSTANT_BYTES as usize];
    push_constant_bytes[0..4].copy_from_slice(&(extent.width as f32).to_ne_bytes());
    push_constant_bytes[4..8].copy_from_slice(&(extent.height as f32).to_ne_bytes());
    push_constant_bytes[8..12].copy_from_slice(&alpha_texture_slot.to_ne_bytes());
    push_constant_bytes[12..16].copy_from_slice(&alpha_texture_mode.shader_code().to_ne_bytes());
    push_constant_bytes[16..20].copy_from_slice(&time_seconds.to_ne_bytes());
    unsafe {
        device.cmd_push_constants(
            command_buffer,
            pipeline_layout,
            vk::ShaderStageFlags::VERTEX | vk::ShaderStageFlags::FRAGMENT,
            0,
            &push_constant_bytes,
        );
    }
}

fn bind_scene_sampled_image_descriptor_heap_for_descriptor_group(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    descriptor_heap_draw: VulkanaliaSceneDescriptorHeapDrawResources<'_>,
    descriptor_group_base_index: u32,
) -> Result<(), String> {
    let image_index = usize::try_from(descriptor_group_base_index).map_err(|_| {
        format!(
            "scene sampled-image descriptor group base index {descriptor_group_base_index} exceeds usize"
        )
    })?;
    let resource_bind = native_vulkan_vulkanalia_descriptor_heap_resource_bind_info_for_image(
        descriptor_heap_draw.resources,
        image_index,
    )?;
    let sampler_bind = native_vulkan_vulkanalia_descriptor_heap_sampler_bind_info_for_image(
        descriptor_heap_draw.resources,
        image_index,
    )?;
    unsafe {
        device.cmd_bind_resource_heap_ext(command_buffer, &resource_bind);
        device.cmd_bind_sampler_heap_ext(command_buffer, &sampler_bind);
    }
    Ok(())
}

fn set_scene_dynamic_viewport_and_scissor(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    extent: vk::Extent2D,
) {
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
    unsafe {
        device.cmd_set_viewport(command_buffer, 0, &[viewport]);
        device.cmd_set_scissor(command_buffer, 0, &[scissor]);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SceneSampledImageActiveRenderingTarget {
    Swapchain,
    EffectTarget(u32),
}

fn scene_color_image_transition(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
    src_stage_mask: vk::PipelineStageFlags2,
    src_access_mask: vk::AccessFlags2,
    dst_stage_mask: vk::PipelineStageFlags2,
    dst_access_mask: vk::AccessFlags2,
) {
    let barrier = vk::ImageMemoryBarrier2::builder()
        .src_stage_mask(src_stage_mask)
        .src_access_mask(src_access_mask)
        .dst_stage_mask(dst_stage_mask)
        .dst_access_mask(dst_access_mask)
        .old_layout(old_layout)
        .new_layout(new_layout)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(native_vulkan_vulkanalia_scene_color_subresource_range())
        .build();
    let barriers = [barrier];
    let dependency = vk::DependencyInfo::builder()
        .image_memory_barriers(&barriers)
        .build();
    unsafe {
        device.cmd_pipeline_barrier2(command_buffer, &dependency);
    }
}

fn begin_scene_color_rendering(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    image_view: vk::ImageView,
    extent: vk::Extent2D,
    load_op: vk::AttachmentLoadOp,
    clear_color: [f32; 4],
) {
    let clear_value = vk::ClearValue {
        color: vk::ClearColorValue {
            float32: clear_color,
        },
    };
    let color_attachment = vk::RenderingAttachmentInfo::builder()
        .image_view(image_view)
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(load_op)
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
    unsafe {
        device.cmd_begin_rendering(command_buffer, &rendering_info);
    }
    set_scene_dynamic_viewport_and_scissor(device, command_buffer, extent);
}

fn end_scene_color_rendering(device: &Device, command_buffer: vk::CommandBuffer) {
    unsafe {
        device.cmd_end_rendering(command_buffer);
    }
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
    effect_target_resources: &[VulkanaliaSceneSampledImageResources],
    vertex_buffer: vk::Buffer,
    index_buffer: vk::Buffer,
    clear_color: [f32; 4],
    elapsed_ms: u64,
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
        if let VulkanaliaSceneSampledImageRenderTarget::EffectTarget { target_index, .. } =
            draw.render_target
            && target_index as usize >= effect_target_resources.len()
        {
            return Err(format!(
                "scene sampled-image draw effect target index {target_index} exceeds effect target resource count {}",
                effect_target_resources.len()
            ));
        }
        match &draw.descriptor_binding {
            VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap {
                descriptor_group_base_index,
                texture_slot_bindings,
            } => {
                let Some(descriptor_heap_draw) = descriptor_heap_draw else {
                    return Err(
                        "scene sampled-image descriptor heap draw requires heap resources"
                            .to_owned(),
                    );
                };
                let descriptor_group_end = *descriptor_group_base_index as usize
                    + SCENE_SAMPLED_IMAGE_TEXTURE_SLOT_BINDING_COUNT;
                if descriptor_group_end > descriptor_heap_draw.resources.plan.image_count {
                    return Err(format!(
                        "scene sampled-image descriptor heap group {}..{} exceeds heap image count {}",
                        descriptor_group_base_index,
                        descriptor_group_end,
                        descriptor_heap_draw.resources.plan.image_count
                    ));
                }
                if texture_slot_bindings.is_empty()
                    || texture_slot_bindings.len() > SCENE_SAMPLED_IMAGE_TEXTURE_SLOT_BINDING_COUNT
                {
                    return Err(format!(
                        "scene sampled-image texture slot count {} exceeds descriptor binding count {}",
                        texture_slot_bindings.len(),
                        SCENE_SAMPLED_IMAGE_TEXTURE_SLOT_BINDING_COUNT
                    ));
                }
                if let Some(alpha_texture_slot) = draw.material.alpha_texture_slot
                    && !texture_slot_bindings
                        .iter()
                        .any(|binding| binding.slot == alpha_texture_slot)
                {
                    return Err(format!(
                        "scene sampled-image alpha texture slot {alpha_texture_slot} has no resource binding"
                    ));
                }
            }
        }
    }
    if descriptor_heap_draw.is_none() {
        return Err("scene sampled-image command requires descriptor heap resources".to_owned());
    }
    if !draw_commands
        .iter()
        .any(|draw| draw.render_target == VulkanaliaSceneSampledImageRenderTarget::Swapchain)
    {
        return Err("scene sampled-image command requires at least one swapchain draw".to_owned());
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
            .flags(vk::CommandBufferUsageFlags::empty())
            .build();
        device
            .begin_command_buffer(command_buffer, &begin_info)
            .map_err(|err| {
                format!("vkBeginCommandBuffer(vulkanalia scene sampled image): {err:?}")
            })?;

        let solid_push_constants = [extent.width as f32, extent.height as f32];
        let solid_push_constant_bytes = std::slice::from_raw_parts(
            solid_push_constants.as_ptr().cast::<u8>(),
            SCENE_FULL_SOLID_QUAD_PUSH_CONSTANT_BYTES as usize,
        );
        let mut active_target: Option<SceneSampledImageActiveRenderingTarget> = None;
        let mut active_extent = extent;
        let mut swapchain_started = false;
        let mut bound_pipeline: Option<VulkanaliaSceneBoundDrawPipeline> = None;
        let mut bound_descriptor_heap_group: Option<u32> = None;
        for draw in &ordered_draws {
            let desired_target = match draw.pipeline {
                VulkanaliaSceneOrderedDrawPipeline::SolidQuad => {
                    SceneSampledImageActiveRenderingTarget::Swapchain
                }
                VulkanaliaSceneOrderedDrawPipeline::SampledImage => {
                    match draw_commands[draw.command_index].render_target {
                        VulkanaliaSceneSampledImageRenderTarget::Swapchain => {
                            SceneSampledImageActiveRenderingTarget::Swapchain
                        }
                        VulkanaliaSceneSampledImageRenderTarget::EffectTarget {
                            target_index,
                            ..
                        } => SceneSampledImageActiveRenderingTarget::EffectTarget(target_index),
                    }
                }
            };
            if active_target != Some(desired_target) {
                if let Some(current_target) = active_target.take() {
                    end_scene_color_rendering(device, command_buffer);
                    if let SceneSampledImageActiveRenderingTarget::EffectTarget(target_index) =
                        current_target
                    {
                        let target = &effect_target_resources[target_index as usize];
                        scene_color_image_transition(
                            device,
                            command_buffer,
                            target.image,
                            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                            vk::PipelineStageFlags2::FRAGMENT_SHADER,
                            vk::AccessFlags2::SHADER_SAMPLED_READ,
                        );
                    } else {
                        scene_color_image_transition(
                            device,
                            command_buffer,
                            swapchain_image,
                            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                            vk::AccessFlags2::COLOR_ATTACHMENT_READ
                                | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                        );
                    }
                    bound_pipeline = None;
                    bound_descriptor_heap_group = None;
                }

                match desired_target {
                    SceneSampledImageActiveRenderingTarget::Swapchain => {
                        let load_op = if swapchain_started {
                            vk::AttachmentLoadOp::LOAD
                        } else {
                            scene_color_image_transition(
                                device,
                                command_buffer,
                                swapchain_image,
                                vk::ImageLayout::UNDEFINED,
                                vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                                vk::PipelineStageFlags2::TOP_OF_PIPE,
                                vk::AccessFlags2::empty(),
                                vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                                vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                            );
                            swapchain_started = true;
                            vk::AttachmentLoadOp::CLEAR
                        };
                        active_extent = extent;
                        begin_scene_color_rendering(
                            device,
                            command_buffer,
                            swapchain_view,
                            active_extent,
                            load_op,
                            clear_color,
                        );
                    }
                    SceneSampledImageActiveRenderingTarget::EffectTarget(target_index) => {
                        let sampled_draw = &draw_commands[draw.command_index];
                        let VulkanaliaSceneSampledImageRenderTarget::EffectTarget { clear, .. } =
                            sampled_draw.render_target
                        else {
                            unreachable!("desired effect target came from sampled draw target");
                        };
                        let target = &effect_target_resources[target_index as usize];
                        active_extent = vk::Extent2D {
                            width: target.snapshot.extent.0,
                            height: target.snapshot.extent.1,
                        };
                        scene_color_image_transition(
                            device,
                            command_buffer,
                            target.image,
                            if clear {
                                vk::ImageLayout::UNDEFINED
                            } else {
                                vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
                            },
                            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                            if clear {
                                vk::PipelineStageFlags2::TOP_OF_PIPE
                            } else {
                                vk::PipelineStageFlags2::FRAGMENT_SHADER
                            },
                            if clear {
                                vk::AccessFlags2::empty()
                            } else {
                                vk::AccessFlags2::SHADER_SAMPLED_READ
                            },
                            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                        );
                        begin_scene_color_rendering(
                            device,
                            command_buffer,
                            target.image_view,
                            active_extent,
                            if clear {
                                vk::AttachmentLoadOp::CLEAR
                            } else {
                                vk::AttachmentLoadOp::LOAD
                            },
                            [0.0, 0.0, 0.0, 0.0],
                        );
                    }
                }
                active_target = Some(desired_target);
            }

            match draw.pipeline {
                VulkanaliaSceneOrderedDrawPipeline::SolidQuad => {
                    let solid_draw = &solid_draw_commands[draw.command_index];
                    let pipeline_key =
                        VulkanaliaSceneBoundDrawPipeline::SolidQuad(solid_draw.blend);
                    if bound_pipeline != Some(pipeline_key) {
                        let solid_resources = solid_quad_draw
                            .as_ref()
                            .expect("solid draw resources present");
                        device.cmd_bind_pipeline(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            native_vulkan_vulkanalia_scene_solid_quad_pipeline(
                                solid_resources.pipeline_resources,
                                solid_draw.blend.mode,
                            ),
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
                        bound_pipeline = Some(pipeline_key);
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
                    let pipeline_key = VulkanaliaSceneBoundDrawPipeline::SampledImage(
                        sampled_draw.material.render_state.blend,
                    );
                    if bound_pipeline != Some(pipeline_key) {
                        device.cmd_bind_pipeline(
                            command_buffer,
                            vk::PipelineBindPoint::GRAPHICS,
                            native_vulkan_vulkanalia_scene_sampled_image_pipeline(
                                pipeline_resources,
                                sampled_draw.material.render_state.blend.mode,
                            ),
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
                        bound_pipeline = Some(pipeline_key);
                    }
                    let VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap {
                        descriptor_group_base_index,
                        ..
                    } = &sampled_draw.descriptor_binding;
                    if bound_descriptor_heap_group != Some(*descriptor_group_base_index) {
                        let descriptor_heap_draw =
                            descriptor_heap_draw.expect("descriptor heap draw resources present");
                        bind_scene_sampled_image_descriptor_heap_for_descriptor_group(
                            device,
                            command_buffer,
                            descriptor_heap_draw,
                            *descriptor_group_base_index,
                        )?;
                        bound_descriptor_heap_group = Some(*descriptor_group_base_index);
                    }
                    push_scene_sampled_image_constants(
                        device,
                        command_buffer,
                        pipeline_resources.pipeline_layout,
                        active_extent,
                        sampled_draw.material.alpha_texture_slot,
                        sampled_draw.material.alpha_texture_mode,
                        elapsed_ms,
                    );
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
        if let Some(current_target) = active_target.take() {
            end_scene_color_rendering(device, command_buffer);
            if let SceneSampledImageActiveRenderingTarget::EffectTarget(target_index) =
                current_target
            {
                let target = &effect_target_resources[target_index as usize];
                scene_color_image_transition(
                    device,
                    command_buffer,
                    target.image,
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    vk::PipelineStageFlags2::FRAGMENT_SHADER,
                    vk::AccessFlags2::SHADER_SAMPLED_READ,
                );
            }
        }

        scene_color_image_transition(
            device,
            command_buffer,
            swapchain_image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::PRESENT_SRC_KHR,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            vk::PipelineStageFlags2::BOTTOM_OF_PIPE,
            vk::AccessFlags2::empty(),
        );

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
        let pipeline_key = native_vulkan_vulkanalia_scene_bound_pipeline_key(
            draw,
            solid_quad_draw.map_or(&[], |draw| draw.draw_commands),
            draw_commands,
        );
        if last_pipeline != Some(pipeline_key) {
            pipeline_bind_count = pipeline_bind_count.saturating_add(1);
            last_pipeline = Some(pipeline_key);
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

const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SOLID_QUAD_PREMULTIPLIED_FRAGMENT_SPIRV: [u32; 164] = [
    119734787, 65536, 524299, 27, 0, 131089, 1, 393227, 1, 1280527431, 1685353262, 808793134, 0,
    196622, 0, 1, 458767, 4, 4, 1852399981, 0, 9, 11, 196624, 4, 7, 196611, 2, 450, 262149, 4,
    1852399981, 0, 327685, 9, 1601467759, 1869377379, 114, 327685, 11, 1667198569, 1919904879, 0,
    262215, 9, 30, 0, 262215, 11, 30, 0, 131091, 2, 196641, 3, 2, 196630, 6, 32, 262167, 7, 6, 4,
    262176, 8, 3, 7, 262203, 8, 9, 3, 262176, 10, 1, 7, 262203, 10, 11, 1, 262167, 12, 6, 3,
    262165, 15, 32, 0, 262187, 15, 16, 3, 262176, 17, 1, 6, 327734, 2, 4, 0, 3, 131320, 5, 262205,
    7, 13, 11, 524367, 12, 14, 13, 13, 0, 1, 2, 327745, 17, 18, 11, 16, 262205, 6, 19, 18, 327822,
    12, 20, 14, 19, 327745, 17, 21, 11, 16, 262205, 6, 22, 21, 327761, 6, 23, 20, 0, 327761, 6, 24,
    20, 1, 327761, 6, 25, 20, 2, 458832, 7, 26, 23, 24, 25, 22, 196670, 9, 26, 65789, 65592,
];

const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_VERTEX_SPIRV: [u32; 506] = [
    0x07230203, 0x00010000, 0x0008000b, 0x0000003d, 0x00000000, 0x00020011, 0x00000001, 0x0006000b,
    0x00000001, 0x4c534c47, 0x6474732e, 0x3035342e, 0x00000000, 0x0003000e, 0x00000000, 0x00000001,
    0x000f000f, 0x00000000, 0x00000004, 0x6e69616d, 0x00000000, 0x0000000b, 0x0000001c, 0x0000002e,
    0x0000002f, 0x00000031, 0x00000032, 0x00000035, 0x00000037, 0x00000039, 0x0000003b, 0x00030003,
    0x00000002, 0x000001c2, 0x00040005, 0x00000004, 0x6e69616d, 0x00000000, 0x00050005, 0x00000009,
    0x6d726f6e, 0x7a696c61, 0x00006465, 0x00050005, 0x0000000b, 0x705f6e69, 0x7469736f, 0x006e6f69,
    0x00050005, 0x0000000e, 0x6e656353, 0x73755065, 0x00000068, 0x00050006, 0x0000000e, 0x00000000,
    0x65747865, 0x0000746e, 0x00080006, 0x0000000e, 0x00000001, 0x68706c61, 0x65745f61, 0x72757478,
    0x6c735f65, 0x0000746f, 0x00080006, 0x0000000e, 0x00000002, 0x68706c61, 0x65745f61, 0x72757478,
    0x6f6d5f65, 0x00006564, 0x00070006, 0x0000000e, 0x00000003, 0x656d6974, 0x6365735f, 0x73646e6f,
    0x00000000, 0x00030005, 0x00000010, 0x00006370, 0x00060005, 0x0000001a, 0x505f6c67, 0x65567265,
    0x78657472, 0x00000000, 0x00060006, 0x0000001a, 0x00000000, 0x505f6c67, 0x7469736f, 0x006e6f69,
    0x00070006, 0x0000001a, 0x00000001, 0x505f6c67, 0x746e696f, 0x657a6953, 0x00000000, 0x00070006,
    0x0000001a, 0x00000002, 0x435f6c67, 0x4470696c, 0x61747369, 0x0065636e, 0x00070006, 0x0000001a,
    0x00000003, 0x435f6c67, 0x446c6c75, 0x61747369, 0x0065636e, 0x00030005, 0x0000001c, 0x00000000,
    0x00040005, 0x0000002e, 0x76755f76, 0x00000000, 0x00040005, 0x0000002f, 0x755f6e69, 0x00000076,
    0x00050005, 0x00000031, 0x66655f76, 0x74636566, 0x0076755f, 0x00060005, 0x00000032, 0x655f6e69,
    0x63656666, 0x76755f74, 0x00000000, 0x00050005, 0x00000035, 0x706f5f76, 0x74696361, 0x00000079,
    0x00050005, 0x00000037, 0x6f5f6e69, 0x69636170, 0x00007974, 0x00040005, 0x00000039, 0x69745f76,
    0x0000746e, 0x00040005, 0x0000003b, 0x745f6e69, 0x00746e69, 0x00040047, 0x0000000b, 0x0000001e,
    0x00000000, 0x00030047, 0x0000000e, 0x00000002, 0x00050048, 0x0000000e, 0x00000000, 0x00000023,
    0x00000000, 0x00050048, 0x0000000e, 0x00000001, 0x00000023, 0x00000008, 0x00050048, 0x0000000e,
    0x00000002, 0x00000023, 0x0000000c, 0x00050048, 0x0000000e, 0x00000003, 0x00000023, 0x00000010,
    0x00030047, 0x0000001a, 0x00000002, 0x00050048, 0x0000001a, 0x00000000, 0x0000000b, 0x00000000,
    0x00050048, 0x0000001a, 0x00000001, 0x0000000b, 0x00000001, 0x00050048, 0x0000001a, 0x00000002,
    0x0000000b, 0x00000003, 0x00050048, 0x0000001a, 0x00000003, 0x0000000b, 0x00000004, 0x00040047,
    0x0000002e, 0x0000001e, 0x00000000, 0x00040047, 0x0000002f, 0x0000001e, 0x00000001, 0x00040047,
    0x00000031, 0x0000001e, 0x00000001, 0x00040047, 0x00000032, 0x0000001e, 0x00000002, 0x00040047,
    0x00000035, 0x0000001e, 0x00000002, 0x00040047, 0x00000037, 0x0000001e, 0x00000003, 0x00040047,
    0x00000039, 0x0000001e, 0x00000003, 0x00040047, 0x0000003b, 0x0000001e, 0x00000004, 0x00020013,
    0x00000002, 0x00030021, 0x00000003, 0x00000002, 0x00030016, 0x00000006, 0x00000020, 0x00040017,
    0x00000007, 0x00000006, 0x00000002, 0x00040020, 0x00000008, 0x00000007, 0x00000007, 0x00040020,
    0x0000000a, 0x00000001, 0x00000007, 0x0004003b, 0x0000000a, 0x0000000b, 0x00000001, 0x00040015,
    0x0000000d, 0x00000020, 0x00000000, 0x0006001e, 0x0000000e, 0x00000007, 0x0000000d, 0x0000000d,
    0x00000006, 0x00040020, 0x0000000f, 0x00000009, 0x0000000e, 0x0004003b, 0x0000000f, 0x00000010,
    0x00000009, 0x00040015, 0x00000011, 0x00000020, 0x00000001, 0x0004002b, 0x00000011, 0x00000012,
    0x00000000, 0x00040020, 0x00000013, 0x00000009, 0x00000007, 0x00040017, 0x00000017, 0x00000006,
    0x00000004, 0x0004002b, 0x0000000d, 0x00000018, 0x00000001, 0x0004001c, 0x00000019, 0x00000006,
    0x00000018, 0x0006001e, 0x0000001a, 0x00000017, 0x00000006, 0x00000019, 0x00000019, 0x00040020,
    0x0000001b, 0x00000003, 0x0000001a, 0x0004003b, 0x0000001b, 0x0000001c, 0x00000003, 0x0004002b,
    0x0000000d, 0x0000001d, 0x00000000, 0x00040020, 0x0000001e, 0x00000007, 0x00000006, 0x0004002b,
    0x00000006, 0x00000021, 0x40000000, 0x0004002b, 0x00000006, 0x00000023, 0x3f800000, 0x0004002b,
    0x00000006, 0x00000029, 0x00000000, 0x00040020, 0x0000002b, 0x00000003, 0x00000017, 0x00040020,
    0x0000002d, 0x00000003, 0x00000007, 0x0004003b, 0x0000002d, 0x0000002e, 0x00000003, 0x0004003b,
    0x0000000a, 0x0000002f, 0x00000001, 0x0004003b, 0x0000002d, 0x00000031, 0x00000003, 0x0004003b,
    0x0000000a, 0x00000032, 0x00000001, 0x00040020, 0x00000034, 0x00000003, 0x00000006, 0x0004003b,
    0x00000034, 0x00000035, 0x00000003, 0x00040020, 0x00000036, 0x00000001, 0x00000006, 0x0004003b,
    0x00000036, 0x00000037, 0x00000001, 0x0004003b, 0x0000002b, 0x00000039, 0x00000003, 0x00040020,
    0x0000003a, 0x00000001, 0x00000017, 0x0004003b, 0x0000003a, 0x0000003b, 0x00000001, 0x00050036,
    0x00000002, 0x00000004, 0x00000000, 0x00000003, 0x000200f8, 0x00000005, 0x0004003b, 0x00000008,
    0x00000009, 0x00000007, 0x0004003d, 0x00000007, 0x0000000c, 0x0000000b, 0x00050041, 0x00000013,
    0x00000014, 0x00000010, 0x00000012, 0x0004003d, 0x00000007, 0x00000015, 0x00000014, 0x00050088,
    0x00000007, 0x00000016, 0x0000000c, 0x00000015, 0x0003003e, 0x00000009, 0x00000016, 0x00050041,
    0x0000001e, 0x0000001f, 0x00000009, 0x0000001d, 0x0004003d, 0x00000006, 0x00000020, 0x0000001f,
    0x00050085, 0x00000006, 0x00000022, 0x00000020, 0x00000021, 0x00050083, 0x00000006, 0x00000024,
    0x00000022, 0x00000023, 0x00050041, 0x0000001e, 0x00000025, 0x00000009, 0x00000018, 0x0004003d,
    0x00000006, 0x00000026, 0x00000025, 0x00050085, 0x00000006, 0x00000027, 0x00000026, 0x00000021,
    0x00050083, 0x00000006, 0x00000028, 0x00000023, 0x00000027, 0x00070050, 0x00000017, 0x0000002a,
    0x00000024, 0x00000028, 0x00000029, 0x00000023, 0x00050041, 0x0000002b, 0x0000002c, 0x0000001c,
    0x00000012, 0x0003003e, 0x0000002c, 0x0000002a, 0x0004003d, 0x00000007, 0x00000030, 0x0000002f,
    0x0003003e, 0x0000002e, 0x00000030, 0x0004003d, 0x00000007, 0x00000033, 0x00000032, 0x0003003e,
    0x00000031, 0x00000033, 0x0004003d, 0x00000006, 0x00000038, 0x00000037, 0x0003003e, 0x00000035,
    0x00000038, 0x0004003d, 0x00000017, 0x0000003c, 0x0000003b, 0x0003003e, 0x00000039, 0x0000003c,
    0x000100fd, 0x00010038,
];

const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_FRAGMENT_SPIRV: [u32; 1944] = [
    0x07230203, 0x00010000, 0x0008000b, 0x0000014f, 0x00000000, 0x00020011, 0x00000001, 0x0006000b,
    0x00000001, 0x4c534c47, 0x6474732e, 0x3035342e, 0x00000000, 0x0003000e, 0x00000000, 0x00000001,
    0x000a000f, 0x00000004, 0x00000004, 0x6e69616d, 0x00000000, 0x000000fe, 0x00000108, 0x00000142,
    0x00000146, 0x0000014d, 0x00030010, 0x00000004, 0x00000007, 0x00030003, 0x00000002, 0x000001c2,
    0x00040005, 0x00000004, 0x6e69616d, 0x00000000, 0x00070005, 0x0000000b, 0x5f776172, 0x68706c61,
    0x616d5f61, 0x76286b73, 0x003b3266, 0x00030005, 0x0000000a, 0x00007675, 0x00060005, 0x0000000e,
    0x68706c61, 0x616d5f61, 0x76286b73, 0x003b3266, 0x00030005, 0x0000000d, 0x00007675, 0x00070005,
    0x00000011, 0x73697269, 0x746f6d5f, 0x5f6e6f69, 0x7366666f, 0x00287465, 0x00060005, 0x00000015,
    0x706d6173, 0x5f64656c, 0x6f6c6f63, 0x00002872, 0x00050005, 0x00000018, 0x6e656353, 0x73755065,
    0x00000068, 0x00050006, 0x00000018, 0x00000000, 0x65747865, 0x0000746e, 0x00080006, 0x00000018,
    0x00000001, 0x68706c61, 0x65745f61, 0x72757478, 0x6c735f65, 0x0000746f, 0x00080006, 0x00000018,
    0x00000002, 0x68706c61, 0x65745f61, 0x72757478, 0x6f6d5f65, 0x00006564, 0x00070006, 0x00000018,
    0x00000003, 0x656d6974, 0x6365735f, 0x73646e6f, 0x00000000, 0x00030005, 0x0000001a, 0x00006370,
    0x00050005, 0x00000028, 0x65545f67, 0x72757478, 0x00003165, 0x00050005, 0x00000035, 0x65545f67,
    0x72757478, 0x00003265, 0x00050005, 0x00000041, 0x65545f67, 0x72757478, 0x00003365, 0x00050005,
    0x0000004d, 0x65545f67, 0x72757478, 0x00003465, 0x00050005, 0x00000059, 0x65545f67, 0x72757478,
    0x00003565, 0x00050005, 0x00000065, 0x65545f67, 0x72757478, 0x00003665, 0x00050005, 0x00000071,
    0x65545f67, 0x72757478, 0x00003765, 0x00040005, 0x00000082, 0x6b73616d, 0x00000000, 0x00040005,
    0x00000083, 0x61726170, 0x0000006d, 0x00040005, 0x00000092, 0x69545f67, 0x0000656d, 0x00040005,
    0x00000097, 0x63535f67, 0x00656c61, 0x00040005, 0x00000099, 0x70535f67, 0x00646565, 0x00040005,
    0x0000009a, 0x6f525f67, 0x00686775, 0x00060005, 0x0000009c, 0x6f4e5f67, 0x41657369, 0x6e756f6d,
    0x00000074, 0x00060005, 0x0000009e, 0x68505f67, 0x4f657361, 0x65736666, 0x00000074, 0x00040005,
    0x000000a0, 0x656d6974, 0x00000000, 0x00040005, 0x000000a6, 0x44776f6c, 0x00000074, 0x00040005,
    0x000000a9, 0x69746f6d, 0x00326e6f, 0x00040005, 0x000000b2, 0x69746f6d, 0x00346e6f, 0x00050005,
    0x000000bd, 0x65766f6d, 0x72617453, 0x00000074, 0x00040005, 0x000000c3, 0x65766f6d, 0x00646e45,
    0x00030005, 0x000000c9, 0x00006164, 0x00040005, 0x000000fc, 0x6b73616d, 0x00000000, 0x00050005,
    0x000000fe, 0x66655f76, 0x74636566, 0x0076755f, 0x00040005, 0x000000ff, 0x61726170, 0x0000006d,
    0x00050005, 0x00000102, 0x73697269, 0x66666f5f, 0x00746573, 0x00050005, 0x00000106, 0x65545f67,
    0x72757478, 0x00003065, 0x00040005, 0x00000108, 0x76755f76, 0x00000000, 0x00050005, 0x00000119,
    0x73697269, 0x73616d5f, 0x0000006b, 0x00040005, 0x0000011a, 0x61726170, 0x0000006d, 0x00050005,
    0x0000011d, 0x73697269, 0x66666f5f, 0x00746573, 0x00040005, 0x00000121, 0x6f6c6f63, 0x00000072,
    0x00040005, 0x00000131, 0x6f6c6f63, 0x00000072, 0x00040005, 0x00000135, 0x61726170, 0x0000006d,
    0x00040005, 0x0000013f, 0x6f6c6f63, 0x00000072, 0x00040005, 0x00000142, 0x69745f76, 0x0000746e,
    0x00050005, 0x00000146, 0x706f5f76, 0x74696361, 0x00000079, 0x00050005, 0x0000014d, 0x5f74756f,
    0x6f6c6f63, 0x00000072, 0x00030047, 0x00000018, 0x00000002, 0x00050048, 0x00000018, 0x00000000,
    0x00000023, 0x00000000, 0x00050048, 0x00000018, 0x00000001, 0x00000023, 0x00000008, 0x00050048,
    0x00000018, 0x00000002, 0x00000023, 0x0000000c, 0x00050048, 0x00000018, 0x00000003, 0x00000023,
    0x00000010, 0x00040047, 0x00000028, 0x00000021, 0x00000001, 0x00040047, 0x00000028, 0x00000022,
    0x00000000, 0x00040047, 0x00000035, 0x00000021, 0x00000002, 0x00040047, 0x00000035, 0x00000022,
    0x00000000, 0x00040047, 0x00000041, 0x00000021, 0x00000003, 0x00040047, 0x00000041, 0x00000022,
    0x00000000, 0x00040047, 0x0000004d, 0x00000021, 0x00000004, 0x00040047, 0x0000004d, 0x00000022,
    0x00000000, 0x00040047, 0x00000059, 0x00000021, 0x00000005, 0x00040047, 0x00000059, 0x00000022,
    0x00000000, 0x00040047, 0x00000065, 0x00000021, 0x00000006, 0x00040047, 0x00000065, 0x00000022,
    0x00000000, 0x00040047, 0x00000071, 0x00000021, 0x00000007, 0x00040047, 0x00000071, 0x00000022,
    0x00000000, 0x00040047, 0x000000fe, 0x0000001e, 0x00000001, 0x00040047, 0x00000106, 0x00000021,
    0x00000000, 0x00040047, 0x00000106, 0x00000022, 0x00000000, 0x00040047, 0x00000108, 0x0000001e,
    0x00000000, 0x00040047, 0x00000142, 0x0000001e, 0x00000003, 0x00040047, 0x00000146, 0x0000001e,
    0x00000002, 0x00040047, 0x0000014d, 0x0000001e, 0x00000000, 0x00020013, 0x00000002, 0x00030021,
    0x00000003, 0x00000002, 0x00030016, 0x00000006, 0x00000020, 0x00040017, 0x00000007, 0x00000006,
    0x00000002, 0x00040020, 0x00000008, 0x00000007, 0x00000007, 0x00040021, 0x00000009, 0x00000006,
    0x00000008, 0x00030021, 0x00000010, 0x00000007, 0x00040017, 0x00000013, 0x00000006, 0x00000004,
    0x00030021, 0x00000014, 0x00000013, 0x00040015, 0x00000017, 0x00000020, 0x00000000, 0x0006001e,
    0x00000018, 0x00000007, 0x00000017, 0x00000017, 0x00000006, 0x00040020, 0x00000019, 0x00000009,
    0x00000018, 0x0004003b, 0x00000019, 0x0000001a, 0x00000009, 0x00040015, 0x0000001b, 0x00000020,
    0x00000001, 0x0004002b, 0x0000001b, 0x0000001c, 0x00000001, 0x00040020, 0x0000001d, 0x00000009,
    0x00000017, 0x0004002b, 0x00000017, 0x00000020, 0x00000001, 0x00020014, 0x00000021, 0x00090019,
    0x00000025, 0x00000006, 0x00000001, 0x00000000, 0x00000000, 0x00000000, 0x00000001, 0x00000000,
    0x0003001b, 0x00000026, 0x00000025, 0x00040020, 0x00000027, 0x00000000, 0x00000026, 0x0004003b,
    0x00000027, 0x00000028, 0x00000000, 0x0004002b, 0x00000017, 0x0000002c, 0x00000000, 0x0004002b,
    0x00000017, 0x00000031, 0x00000002, 0x0004003b, 0x00000027, 0x00000035, 0x00000000, 0x0004002b,
    0x00000017, 0x0000003d, 0x00000003, 0x0004003b, 0x00000027, 0x00000041, 0x00000000, 0x0004002b,
    0x00000017, 0x00000049, 0x00000004, 0x0004003b, 0x00000027, 0x0000004d, 0x00000000, 0x0004002b,
    0x00000017, 0x00000055, 0x00000005, 0x0004003b, 0x00000027, 0x00000059, 0x00000000, 0x0004002b,
    0x00000017, 0x00000061, 0x00000006, 0x0004003b, 0x00000027, 0x00000065, 0x00000000, 0x0004002b,
    0x00000017, 0x0000006d, 0x00000007, 0x0004003b, 0x00000027, 0x00000071, 0x00000000, 0x0004002b,
    0x00000006, 0x00000077, 0x3f800000, 0x0004002b, 0x00000017, 0x0000007c, 0xffffffff, 0x00040020,
    0x00000081, 0x00000007, 0x00000006, 0x0004002b, 0x0000001b, 0x00000086, 0x00000002, 0x0004002b,
    0x0000001b, 0x00000093, 0x00000003, 0x00040020, 0x00000094, 0x00000009, 0x00000006, 0x0005002c,
    0x00000007, 0x00000098, 0x00000077, 0x00000077, 0x0004002b, 0x00000006, 0x0000009b, 0x3e4ccccd,
    0x0004002b, 0x00000006, 0x0000009d, 0x3f000000, 0x0004002b, 0x00000006, 0x0000009f, 0x00000000,
    0x0004002b, 0x00000006, 0x000000aa, 0x3ff33333, 0x0005002c, 0x00000007, 0x000000ac, 0x0000009f,
    0x00000077, 0x00040020, 0x000000b1, 0x00000007, 0x00000013, 0x0004002b, 0x00000006, 0x000000b3,
    0x40200000, 0x0007002c, 0x00000013, 0x000000b5, 0x0000009f, 0x0000009f, 0x00000077, 0x00000077,
    0x0004002b, 0x00000006, 0x000000b9, 0x40000000, 0x0007002c, 0x00000013, 0x000000ba, 0x00000077,
    0x000000b9, 0x00000077, 0x000000b9, 0x0004002b, 0x00000006, 0x000000d0, 0x40490fdb, 0x0004002b,
    0x00000006, 0x000000d3, 0xbf000000, 0x0004002b, 0x00000006, 0x000000ea, 0x3a83126f, 0x00040020,
    0x000000fd, 0x00000001, 0x00000007, 0x0004003b, 0x000000fd, 0x000000fe, 0x00000001, 0x0004003b,
    0x00000027, 0x00000106, 0x00000000, 0x0004003b, 0x000000fd, 0x00000108, 0x00000001, 0x00040020,
    0x00000141, 0x00000001, 0x00000013, 0x0004003b, 0x00000141, 0x00000142, 0x00000001, 0x00040020,
    0x00000145, 0x00000001, 0x00000006, 0x0004003b, 0x00000145, 0x00000146, 0x00000001, 0x00040020,
    0x0000014c, 0x00000003, 0x00000013, 0x0004003b, 0x0000014c, 0x0000014d, 0x00000003, 0x00050036,
    0x00000002, 0x00000004, 0x00000000, 0x00000003, 0x000200f8, 0x00000005, 0x0004003b, 0x000000b1,
    0x0000013f, 0x00000007, 0x00040039, 0x00000013, 0x00000140, 0x00000015, 0x0004003d, 0x00000013,
    0x00000143, 0x00000142, 0x00050085, 0x00000013, 0x00000144, 0x00000140, 0x00000143, 0x0003003e,
    0x0000013f, 0x00000144, 0x0004003d, 0x00000006, 0x00000147, 0x00000146, 0x00050041, 0x00000081,
    0x00000148, 0x0000013f, 0x0000003d, 0x0004003d, 0x00000006, 0x00000149, 0x00000148, 0x00050085,
    0x00000006, 0x0000014a, 0x00000149, 0x00000147, 0x00050041, 0x00000081, 0x0000014b, 0x0000013f,
    0x0000003d, 0x0003003e, 0x0000014b, 0x0000014a, 0x0004003d, 0x00000013, 0x0000014e, 0x0000013f,
    0x0003003e, 0x0000014d, 0x0000014e, 0x000100fd, 0x00010038, 0x00050036, 0x00000006, 0x0000000b,
    0x00000000, 0x00000009, 0x00030037, 0x00000008, 0x0000000a, 0x000200f8, 0x0000000c, 0x00050041,
    0x0000001d, 0x0000001e, 0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x0000001f, 0x0000001e,
    0x000500aa, 0x00000021, 0x00000022, 0x0000001f, 0x00000020, 0x000300f7, 0x00000024, 0x00000000,
    0x000400fa, 0x00000022, 0x00000023, 0x00000024, 0x000200f8, 0x00000023, 0x0004003d, 0x00000026,
    0x00000029, 0x00000028, 0x0004003d, 0x00000007, 0x0000002a, 0x0000000a, 0x00050057, 0x00000013,
    0x0000002b, 0x00000029, 0x0000002a, 0x00050051, 0x00000006, 0x0000002d, 0x0000002b, 0x00000000,
    0x000200fe, 0x0000002d, 0x000200f8, 0x00000024, 0x00050041, 0x0000001d, 0x0000002f, 0x0000001a,
    0x0000001c, 0x0004003d, 0x00000017, 0x00000030, 0x0000002f, 0x000500aa, 0x00000021, 0x00000032,
    0x00000030, 0x00000031, 0x000300f7, 0x00000034, 0x00000000, 0x000400fa, 0x00000032, 0x00000033,
    0x00000034, 0x000200f8, 0x00000033, 0x0004003d, 0x00000026, 0x00000036, 0x00000035, 0x0004003d,
    0x00000007, 0x00000037, 0x0000000a, 0x00050057, 0x00000013, 0x00000038, 0x00000036, 0x00000037,
    0x00050051, 0x00000006, 0x00000039, 0x00000038, 0x00000000, 0x000200fe, 0x00000039, 0x000200f8,
    0x00000034, 0x00050041, 0x0000001d, 0x0000003b, 0x0000001a, 0x0000001c, 0x0004003d, 0x00000017,
    0x0000003c, 0x0000003b, 0x000500aa, 0x00000021, 0x0000003e, 0x0000003c, 0x0000003d, 0x000300f7,
    0x00000040, 0x00000000, 0x000400fa, 0x0000003e, 0x0000003f, 0x00000040, 0x000200f8, 0x0000003f,
    0x0004003d, 0x00000026, 0x00000042, 0x00000041, 0x0004003d, 0x00000007, 0x00000043, 0x0000000a,
    0x00050057, 0x00000013, 0x00000044, 0x00000042, 0x00000043, 0x00050051, 0x00000006, 0x00000045,
    0x00000044, 0x00000000, 0x000200fe, 0x00000045, 0x000200f8, 0x00000040, 0x00050041, 0x0000001d,
    0x00000047, 0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x00000048, 0x00000047, 0x000500aa,
    0x00000021, 0x0000004a, 0x00000048, 0x00000049, 0x000300f7, 0x0000004c, 0x00000000, 0x000400fa,
    0x0000004a, 0x0000004b, 0x0000004c, 0x000200f8, 0x0000004b, 0x0004003d, 0x00000026, 0x0000004e,
    0x0000004d, 0x0004003d, 0x00000007, 0x0000004f, 0x0000000a, 0x00050057, 0x00000013, 0x00000050,
    0x0000004e, 0x0000004f, 0x00050051, 0x00000006, 0x00000051, 0x00000050, 0x00000000, 0x000200fe,
    0x00000051, 0x000200f8, 0x0000004c, 0x00050041, 0x0000001d, 0x00000053, 0x0000001a, 0x0000001c,
    0x0004003d, 0x00000017, 0x00000054, 0x00000053, 0x000500aa, 0x00000021, 0x00000056, 0x00000054,
    0x00000055, 0x000300f7, 0x00000058, 0x00000000, 0x000400fa, 0x00000056, 0x00000057, 0x00000058,
    0x000200f8, 0x00000057, 0x0004003d, 0x00000026, 0x0000005a, 0x00000059, 0x0004003d, 0x00000007,
    0x0000005b, 0x0000000a, 0x00050057, 0x00000013, 0x0000005c, 0x0000005a, 0x0000005b, 0x00050051,
    0x00000006, 0x0000005d, 0x0000005c, 0x00000000, 0x000200fe, 0x0000005d, 0x000200f8, 0x00000058,
    0x00050041, 0x0000001d, 0x0000005f, 0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x00000060,
    0x0000005f, 0x000500aa, 0x00000021, 0x00000062, 0x00000060, 0x00000061, 0x000300f7, 0x00000064,
    0x00000000, 0x000400fa, 0x00000062, 0x00000063, 0x00000064, 0x000200f8, 0x00000063, 0x0004003d,
    0x00000026, 0x00000066, 0x00000065, 0x0004003d, 0x00000007, 0x00000067, 0x0000000a, 0x00050057,
    0x00000013, 0x00000068, 0x00000066, 0x00000067, 0x00050051, 0x00000006, 0x00000069, 0x00000068,
    0x00000000, 0x000200fe, 0x00000069, 0x000200f8, 0x00000064, 0x00050041, 0x0000001d, 0x0000006b,
    0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x0000006c, 0x0000006b, 0x000500aa, 0x00000021,
    0x0000006e, 0x0000006c, 0x0000006d, 0x000300f7, 0x00000070, 0x00000000, 0x000400fa, 0x0000006e,
    0x0000006f, 0x00000070, 0x000200f8, 0x0000006f, 0x0004003d, 0x00000026, 0x00000072, 0x00000071,
    0x0004003d, 0x00000007, 0x00000073, 0x0000000a, 0x00050057, 0x00000013, 0x00000074, 0x00000072,
    0x00000073, 0x00050051, 0x00000006, 0x00000075, 0x00000074, 0x00000000, 0x000200fe, 0x00000075,
    0x000200f8, 0x00000070, 0x000200fe, 0x00000077, 0x00010038, 0x00050036, 0x00000006, 0x0000000e,
    0x00000000, 0x00000009, 0x00030037, 0x00000008, 0x0000000d, 0x000200f8, 0x0000000f, 0x0004003b,
    0x00000081, 0x00000082, 0x00000007, 0x0004003b, 0x00000008, 0x00000083, 0x00000007, 0x00050041,
    0x0000001d, 0x0000007a, 0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x0000007b, 0x0000007a,
    0x000500aa, 0x00000021, 0x0000007d, 0x0000007b, 0x0000007c, 0x000300f7, 0x0000007f, 0x00000000,
    0x000400fa, 0x0000007d, 0x0000007e, 0x0000007f, 0x000200f8, 0x0000007e, 0x000200fe, 0x00000077,
    0x000200f8, 0x0000007f, 0x0004003d, 0x00000007, 0x00000084, 0x0000000d, 0x0003003e, 0x00000083,
    0x00000084, 0x00050039, 0x00000006, 0x00000085, 0x0000000b, 0x00000083, 0x0003003e, 0x00000082,
    0x00000085, 0x00050041, 0x0000001d, 0x00000087, 0x0000001a, 0x00000086, 0x0004003d, 0x00000017,
    0x00000088, 0x00000087, 0x000500aa, 0x00000021, 0x00000089, 0x00000088, 0x00000020, 0x000300f7,
    0x0000008b, 0x00000000, 0x000400fa, 0x00000089, 0x0000008a, 0x0000008b, 0x000200f8, 0x0000008a,
    0x0004003d, 0x00000006, 0x0000008c, 0x00000082, 0x00050083, 0x00000006, 0x0000008d, 0x00000077,
    0x0000008c, 0x000200fe, 0x0000008d, 0x000200f8, 0x0000008b, 0x0004003d, 0x00000006, 0x0000008f,
    0x00000082, 0x000200fe, 0x0000008f, 0x00010038, 0x00050036, 0x00000007, 0x00000011, 0x00000000,
    0x00000010, 0x000200f8, 0x00000012, 0x0004003b, 0x00000081, 0x00000092, 0x00000007, 0x0004003b,
    0x00000008, 0x00000097, 0x00000007, 0x0004003b, 0x00000081, 0x00000099, 0x00000007, 0x0004003b,
    0x00000081, 0x0000009a, 0x00000007, 0x0004003b, 0x00000081, 0x0000009c, 0x00000007, 0x0004003b,
    0x00000081, 0x0000009e, 0x00000007, 0x0004003b, 0x00000081, 0x000000a0, 0x00000007, 0x0004003b,
    0x00000081, 0x000000a6, 0x00000007, 0x0004003b, 0x00000008, 0x000000a9, 0x00000007, 0x0004003b,
    0x000000b1, 0x000000b2, 0x00000007, 0x0004003b, 0x00000008, 0x000000bd, 0x00000007, 0x0004003b,
    0x00000008, 0x000000c3, 0x00000007, 0x0004003b, 0x00000008, 0x000000c9, 0x00000007, 0x00050041,
    0x00000094, 0x00000095, 0x0000001a, 0x00000093, 0x0004003d, 0x00000006, 0x00000096, 0x00000095,
    0x0003003e, 0x00000092, 0x00000096, 0x0003003e, 0x00000097, 0x00000098, 0x0003003e, 0x00000099,
    0x00000077, 0x0003003e, 0x0000009a, 0x0000009b, 0x0003003e, 0x0000009c, 0x0000009d, 0x0003003e,
    0x0000009e, 0x0000009f, 0x0004003d, 0x00000006, 0x000000a1, 0x00000092, 0x0004003d, 0x00000006,
    0x000000a2, 0x00000099, 0x00050085, 0x00000006, 0x000000a3, 0x000000a1, 0x000000a2, 0x0004003d,
    0x00000006, 0x000000a4, 0x0000009e, 0x00050081, 0x00000006, 0x000000a5, 0x000000a3, 0x000000a4,
    0x0003003e, 0x000000a0, 0x000000a5, 0x0004003d, 0x00000006, 0x000000a7, 0x000000a0, 0x0006000c,
    0x00000006, 0x000000a8, 0x00000001, 0x00000008, 0x000000a7, 0x0003003e, 0x000000a6, 0x000000a8,
    0x0004003d, 0x00000006, 0x000000ab, 0x000000a6, 0x00050050, 0x00000007, 0x000000ad, 0x000000ab,
    0x000000ab, 0x00050081, 0x00000007, 0x000000ae, 0x000000ad, 0x000000ac, 0x0005008e, 0x00000007,
    0x000000af, 0x000000ae, 0x000000aa, 0x0006000c, 0x00000007, 0x000000b0, 0x00000001, 0x0000000d,
    0x000000af, 0x0003003e, 0x000000a9, 0x000000b0, 0x0004003d, 0x00000006, 0x000000b4, 0x000000a6,
    0x00070050, 0x00000013, 0x000000b6, 0x000000b4, 0x000000b4, 0x000000b4, 0x000000b4, 0x00050081,
    0x00000013, 0x000000b7, 0x000000b6, 0x000000b5, 0x0005008e, 0x00000013, 0x000000b8, 0x000000b7,
    0x000000b3, 0x00050081, 0x00000013, 0x000000bb, 0x000000b8, 0x000000ba, 0x0006000c, 0x00000013,
    0x000000bc, 0x00000001, 0x0000000d, 0x000000bb, 0x0003003e, 0x000000b2, 0x000000bc, 0x0004003d,
    0x00000007, 0x000000be, 0x000000a9, 0x0007004f, 0x00000007, 0x000000bf, 0x000000be, 0x000000be,
    0x00000000, 0x00000000, 0x0004003d, 0x00000013, 0x000000c0, 0x000000b2, 0x0007004f, 0x00000007,
    0x000000c1, 0x000000c0, 0x000000c0, 0x00000000, 0x00000001, 0x00050081, 0x00000007, 0x000000c2,
    0x000000bf, 0x000000c1, 0x0003003e, 0x000000bd, 0x000000c2, 0x0004003d, 0x00000007, 0x000000c4,
    0x000000a9, 0x0007004f, 0x00000007, 0x000000c5, 0x000000c4, 0x000000c4, 0x00000001, 0x00000001,
    0x0004003d, 0x00000013, 0x000000c6, 0x000000b2, 0x0007004f, 0x00000007, 0x000000c7, 0x000000c6,
    0x000000c6, 0x00000002, 0x00000003, 0x00050081, 0x00000007, 0x000000c8, 0x000000c5, 0x000000c7,
    0x0003003e, 0x000000c3, 0x000000c8, 0x0004003d, 0x00000007, 0x000000ca, 0x000000bd, 0x0004003d,
    0x00000007, 0x000000cb, 0x000000c3, 0x0004003d, 0x00000006, 0x000000cc, 0x0000009a, 0x00050083,
    0x00000006, 0x000000cd, 0x00000077, 0x000000cc, 0x0004003d, 0x00000006, 0x000000ce, 0x000000a0,
    0x0006000c, 0x00000006, 0x000000cf, 0x00000001, 0x0000000a, 0x000000ce, 0x00050085, 0x00000006,
    0x000000d1, 0x000000cf, 0x000000d0, 0x0006000c, 0x00000006, 0x000000d2, 0x00000001, 0x0000000e,
    0x000000d1, 0x00050085, 0x00000006, 0x000000d4, 0x000000d2, 0x000000d3, 0x00050081, 0x00000006,
    0x000000d5, 0x000000d4, 0x0000009d, 0x0008000c, 0x00000006, 0x000000d6, 0x00000001, 0x00000031,
    0x000000cd, 0x00000077, 0x000000d5, 0x00050050, 0x00000007, 0x000000d7, 0x000000d6, 0x000000d6,
    0x0008000c, 0x00000007, 0x000000d8, 0x00000001, 0x0000002e, 0x000000ca, 0x000000cb, 0x000000d7,
    0x0003003e, 0x000000c9, 0x000000d8, 0x0004003d, 0x00000006, 0x000000d9, 0x000000a0, 0x0006000c,
    0x00000006, 0x000000da, 0x00000001, 0x0000000d, 0x000000d9, 0x0004003d, 0x00000006, 0x000000db,
    0x0000009c, 0x00050085, 0x00000006, 0x000000dc, 0x000000da, 0x000000db, 0x00050041, 0x00000081,
    0x000000dd, 0x000000c9, 0x0000002c, 0x0004003d, 0x00000006, 0x000000de, 0x000000dd, 0x00050081,
    0x00000006, 0x000000df, 0x000000de, 0x000000dc, 0x00050041, 0x00000081, 0x000000e0, 0x000000c9,
    0x0000002c, 0x0003003e, 0x000000e0, 0x000000df, 0x0004003d, 0x00000006, 0x000000e1, 0x000000a0,
    0x0006000c, 0x00000006, 0x000000e2, 0x00000001, 0x0000000e, 0x000000e1, 0x0004003d, 0x00000006,
    0x000000e3, 0x0000009c, 0x00050085, 0x00000006, 0x000000e4, 0x000000e2, 0x000000e3, 0x00050041,
    0x00000081, 0x000000e5, 0x000000c9, 0x00000020, 0x0004003d, 0x00000006, 0x000000e6, 0x000000e5,
    0x00050081, 0x00000006, 0x000000e7, 0x000000e6, 0x000000e4, 0x00050041, 0x00000081, 0x000000e8,
    0x000000c9, 0x00000020, 0x0003003e, 0x000000e8, 0x000000e7, 0x0004003d, 0x00000007, 0x000000e9,
    0x00000097, 0x0005008e, 0x00000007, 0x000000eb, 0x000000e9, 0x000000ea, 0x0004003d, 0x00000007,
    0x000000ec, 0x000000c9, 0x00050085, 0x00000007, 0x000000ed, 0x000000ec, 0x000000eb, 0x0003003e,
    0x000000c9, 0x000000ed, 0x0004003d, 0x00000007, 0x000000ee, 0x000000c9, 0x000200fe, 0x000000ee,
    0x00010038, 0x00050036, 0x00000013, 0x00000015, 0x00000000, 0x00000014, 0x000200f8, 0x00000016,
    0x0004003b, 0x00000081, 0x000000fc, 0x00000007, 0x0004003b, 0x00000008, 0x000000ff, 0x00000007,
    0x0004003b, 0x00000008, 0x00000102, 0x00000007, 0x0004003b, 0x00000081, 0x00000119, 0x00000007,
    0x0004003b, 0x00000008, 0x0000011a, 0x00000007, 0x0004003b, 0x00000008, 0x0000011d, 0x00000007,
    0x0004003b, 0x000000b1, 0x00000121, 0x00000007, 0x0004003b, 0x000000b1, 0x00000131, 0x00000007,
    0x0004003b, 0x00000008, 0x00000135, 0x00000007, 0x00050041, 0x0000001d, 0x000000f1, 0x0000001a,
    0x0000001c, 0x0004003d, 0x00000017, 0x000000f2, 0x000000f1, 0x000500ab, 0x00000021, 0x000000f3,
    0x000000f2, 0x0000007c, 0x000300f7, 0x000000f5, 0x00000000, 0x000400fa, 0x000000f3, 0x000000f4,
    0x000000f5, 0x000200f8, 0x000000f4, 0x00050041, 0x0000001d, 0x000000f6, 0x0000001a, 0x00000086,
    0x0004003d, 0x00000017, 0x000000f7, 0x000000f6, 0x000500aa, 0x00000021, 0x000000f8, 0x000000f7,
    0x00000031, 0x000200f9, 0x000000f5, 0x000200f8, 0x000000f5, 0x000700f5, 0x00000021, 0x000000f9,
    0x000000f3, 0x00000016, 0x000000f8, 0x000000f4, 0x000300f7, 0x000000fb, 0x00000000, 0x000400fa,
    0x000000f9, 0x000000fa, 0x000000fb, 0x000200f8, 0x000000fa, 0x0004003d, 0x00000007, 0x00000100,
    0x000000fe, 0x0003003e, 0x000000ff, 0x00000100, 0x00050039, 0x00000006, 0x00000101, 0x0000000b,
    0x000000ff, 0x0003003e, 0x000000fc, 0x00000101, 0x00040039, 0x00000007, 0x00000103, 0x00000011,
    0x0004003d, 0x00000006, 0x00000104, 0x000000fc, 0x0005008e, 0x00000007, 0x00000105, 0x00000103,
    0x00000104, 0x0003003e, 0x00000102, 0x00000105, 0x0004003d, 0x00000026, 0x00000107, 0x00000106,
    0x0004003d, 0x00000007, 0x00000109, 0x00000108, 0x0004003d, 0x00000007, 0x0000010a, 0x00000102,
    0x00050081, 0x00000007, 0x0000010b, 0x00000109, 0x0000010a, 0x00050057, 0x00000013, 0x0000010c,
    0x00000107, 0x0000010b, 0x000200fe, 0x0000010c, 0x000200f8, 0x000000fb, 0x00050041, 0x0000001d,
    0x0000010e, 0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x0000010f, 0x0000010e, 0x000500ab,
    0x00000021, 0x00000110, 0x0000010f, 0x0000007c, 0x000300f7, 0x00000112, 0x00000000, 0x000400fa,
    0x00000110, 0x00000111, 0x00000112, 0x000200f8, 0x00000111, 0x00050041, 0x0000001d, 0x00000113,
    0x0000001a, 0x00000086, 0x0004003d, 0x00000017, 0x00000114, 0x00000113, 0x000500aa, 0x00000021,
    0x00000115, 0x00000114, 0x00000049, 0x000200f9, 0x00000112, 0x000200f8, 0x00000112, 0x000700f5,
    0x00000021, 0x00000116, 0x00000110, 0x000000fb, 0x00000115, 0x00000111, 0x000300f7, 0x00000118,
    0x00000000, 0x000400fa, 0x00000116, 0x00000117, 0x00000118, 0x000200f8, 0x00000117, 0x0004003d,
    0x00000007, 0x0000011b, 0x000000fe, 0x0003003e, 0x0000011a, 0x0000011b, 0x00050039, 0x00000006,
    0x0000011c, 0x0000000b, 0x0000011a, 0x0003003e, 0x00000119, 0x0000011c, 0x00040039, 0x00000007,
    0x0000011e, 0x00000011, 0x0004003d, 0x00000006, 0x0000011f, 0x00000119, 0x0005008e, 0x00000007,
    0x00000120, 0x0000011e, 0x0000011f, 0x0003003e, 0x0000011d, 0x00000120, 0x0004003d, 0x00000026,
    0x00000122, 0x00000106, 0x0004003d, 0x00000007, 0x00000123, 0x00000108, 0x0004003d, 0x00000007,
    0x00000124, 0x0000011d, 0x00050081, 0x00000007, 0x00000125, 0x00000123, 0x00000124, 0x00050057,
    0x00000013, 0x00000126, 0x00000122, 0x00000125, 0x0003003e, 0x00000121, 0x00000126, 0x0004003d,
    0x00000026, 0x00000127, 0x00000035, 0x0004003d, 0x00000007, 0x00000128, 0x000000fe, 0x00050057,
    0x00000013, 0x00000129, 0x00000127, 0x00000128, 0x00050051, 0x00000006, 0x0000012a, 0x00000129,
    0x00000000, 0x00050041, 0x00000081, 0x0000012b, 0x00000121, 0x0000003d, 0x0004003d, 0x00000006,
    0x0000012c, 0x0000012b, 0x00050085, 0x00000006, 0x0000012d, 0x0000012c, 0x0000012a, 0x00050041,
    0x00000081, 0x0000012e, 0x00000121, 0x0000003d, 0x0003003e, 0x0000012e, 0x0000012d, 0x0004003d,
    0x00000013, 0x0000012f, 0x00000121, 0x000200fe, 0x0000012f, 0x000200f8, 0x00000118, 0x0004003d,
    0x00000026, 0x00000132, 0x00000106, 0x0004003d, 0x00000007, 0x00000133, 0x00000108, 0x00050057,
    0x00000013, 0x00000134, 0x00000132, 0x00000133, 0x0003003e, 0x00000131, 0x00000134, 0x0004003d,
    0x00000007, 0x00000136, 0x000000fe, 0x0003003e, 0x00000135, 0x00000136, 0x00050039, 0x00000006,
    0x00000137, 0x0000000e, 0x00000135, 0x00050041, 0x00000081, 0x00000138, 0x00000131, 0x0000003d,
    0x0004003d, 0x00000006, 0x00000139, 0x00000138, 0x00050085, 0x00000006, 0x0000013a, 0x00000139,
    0x00000137, 0x00050041, 0x00000081, 0x0000013b, 0x00000131, 0x0000003d, 0x0003003e, 0x0000013b,
    0x0000013a, 0x0004003d, 0x00000013, 0x0000013c, 0x00000131, 0x000200fe, 0x0000013c, 0x00010038,
];

const NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_PREMULTIPLIED_FRAGMENT_SPIRV: [u32; 2001] = [
    0x07230203, 0x00010000, 0x0008000b, 0x0000015a, 0x00000000, 0x00020011, 0x00000001, 0x0006000b,
    0x00000001, 0x4c534c47, 0x6474732e, 0x3035342e, 0x00000000, 0x0003000e, 0x00000000, 0x00000001,
    0x000a000f, 0x00000004, 0x00000004, 0x6e69616d, 0x00000000, 0x000000fe, 0x00000108, 0x00000142,
    0x00000146, 0x0000014d, 0x00030010, 0x00000004, 0x00000007, 0x00030003, 0x00000002, 0x000001c2,
    0x00040005, 0x00000004, 0x6e69616d, 0x00000000, 0x00070005, 0x0000000b, 0x5f776172, 0x68706c61,
    0x616d5f61, 0x76286b73, 0x003b3266, 0x00030005, 0x0000000a, 0x00007675, 0x00060005, 0x0000000e,
    0x68706c61, 0x616d5f61, 0x76286b73, 0x003b3266, 0x00030005, 0x0000000d, 0x00007675, 0x00070005,
    0x00000011, 0x73697269, 0x746f6d5f, 0x5f6e6f69, 0x7366666f, 0x00287465, 0x00060005, 0x00000015,
    0x706d6173, 0x5f64656c, 0x6f6c6f63, 0x00002872, 0x00050005, 0x00000018, 0x6e656353, 0x73755065,
    0x00000068, 0x00050006, 0x00000018, 0x00000000, 0x65747865, 0x0000746e, 0x00080006, 0x00000018,
    0x00000001, 0x68706c61, 0x65745f61, 0x72757478, 0x6c735f65, 0x0000746f, 0x00080006, 0x00000018,
    0x00000002, 0x68706c61, 0x65745f61, 0x72757478, 0x6f6d5f65, 0x00006564, 0x00070006, 0x00000018,
    0x00000003, 0x656d6974, 0x6365735f, 0x73646e6f, 0x00000000, 0x00030005, 0x0000001a, 0x00006370,
    0x00050005, 0x00000028, 0x65545f67, 0x72757478, 0x00003165, 0x00050005, 0x00000035, 0x65545f67,
    0x72757478, 0x00003265, 0x00050005, 0x00000041, 0x65545f67, 0x72757478, 0x00003365, 0x00050005,
    0x0000004d, 0x65545f67, 0x72757478, 0x00003465, 0x00050005, 0x00000059, 0x65545f67, 0x72757478,
    0x00003565, 0x00050005, 0x00000065, 0x65545f67, 0x72757478, 0x00003665, 0x00050005, 0x00000071,
    0x65545f67, 0x72757478, 0x00003765, 0x00040005, 0x00000082, 0x6b73616d, 0x00000000, 0x00040005,
    0x00000083, 0x61726170, 0x0000006d, 0x00040005, 0x00000092, 0x69545f67, 0x0000656d, 0x00040005,
    0x00000097, 0x63535f67, 0x00656c61, 0x00040005, 0x00000099, 0x70535f67, 0x00646565, 0x00040005,
    0x0000009a, 0x6f525f67, 0x00686775, 0x00060005, 0x0000009c, 0x6f4e5f67, 0x41657369, 0x6e756f6d,
    0x00000074, 0x00060005, 0x0000009e, 0x68505f67, 0x4f657361, 0x65736666, 0x00000074, 0x00040005,
    0x000000a0, 0x656d6974, 0x00000000, 0x00040005, 0x000000a6, 0x44776f6c, 0x00000074, 0x00040005,
    0x000000a9, 0x69746f6d, 0x00326e6f, 0x00040005, 0x000000b2, 0x69746f6d, 0x00346e6f, 0x00050005,
    0x000000bd, 0x65766f6d, 0x72617453, 0x00000074, 0x00040005, 0x000000c3, 0x65766f6d, 0x00646e45,
    0x00030005, 0x000000c9, 0x00006164, 0x00040005, 0x000000fc, 0x6b73616d, 0x00000000, 0x00050005,
    0x000000fe, 0x66655f76, 0x74636566, 0x0076755f, 0x00040005, 0x000000ff, 0x61726170, 0x0000006d,
    0x00050005, 0x00000102, 0x73697269, 0x66666f5f, 0x00746573, 0x00050005, 0x00000106, 0x65545f67,
    0x72757478, 0x00003065, 0x00040005, 0x00000108, 0x76755f76, 0x00000000, 0x00050005, 0x00000119,
    0x73697269, 0x73616d5f, 0x0000006b, 0x00040005, 0x0000011a, 0x61726170, 0x0000006d, 0x00050005,
    0x0000011d, 0x73697269, 0x66666f5f, 0x00746573, 0x00040005, 0x00000121, 0x6f6c6f63, 0x00000072,
    0x00040005, 0x00000131, 0x6f6c6f63, 0x00000072, 0x00040005, 0x00000135, 0x61726170, 0x0000006d,
    0x00040005, 0x0000013f, 0x6f6c6f63, 0x00000072, 0x00040005, 0x00000142, 0x69745f76, 0x0000746e,
    0x00050005, 0x00000146, 0x706f5f76, 0x74696361, 0x00000079, 0x00050005, 0x0000014d, 0x5f74756f,
    0x6f6c6f63, 0x00000072, 0x00030047, 0x00000018, 0x00000002, 0x00050048, 0x00000018, 0x00000000,
    0x00000023, 0x00000000, 0x00050048, 0x00000018, 0x00000001, 0x00000023, 0x00000008, 0x00050048,
    0x00000018, 0x00000002, 0x00000023, 0x0000000c, 0x00050048, 0x00000018, 0x00000003, 0x00000023,
    0x00000010, 0x00040047, 0x00000028, 0x00000021, 0x00000001, 0x00040047, 0x00000028, 0x00000022,
    0x00000000, 0x00040047, 0x00000035, 0x00000021, 0x00000002, 0x00040047, 0x00000035, 0x00000022,
    0x00000000, 0x00040047, 0x00000041, 0x00000021, 0x00000003, 0x00040047, 0x00000041, 0x00000022,
    0x00000000, 0x00040047, 0x0000004d, 0x00000021, 0x00000004, 0x00040047, 0x0000004d, 0x00000022,
    0x00000000, 0x00040047, 0x00000059, 0x00000021, 0x00000005, 0x00040047, 0x00000059, 0x00000022,
    0x00000000, 0x00040047, 0x00000065, 0x00000021, 0x00000006, 0x00040047, 0x00000065, 0x00000022,
    0x00000000, 0x00040047, 0x00000071, 0x00000021, 0x00000007, 0x00040047, 0x00000071, 0x00000022,
    0x00000000, 0x00040047, 0x000000fe, 0x0000001e, 0x00000001, 0x00040047, 0x00000106, 0x00000021,
    0x00000000, 0x00040047, 0x00000106, 0x00000022, 0x00000000, 0x00040047, 0x00000108, 0x0000001e,
    0x00000000, 0x00040047, 0x00000142, 0x0000001e, 0x00000003, 0x00040047, 0x00000146, 0x0000001e,
    0x00000002, 0x00040047, 0x0000014d, 0x0000001e, 0x00000000, 0x00020013, 0x00000002, 0x00030021,
    0x00000003, 0x00000002, 0x00030016, 0x00000006, 0x00000020, 0x00040017, 0x00000007, 0x00000006,
    0x00000002, 0x00040020, 0x00000008, 0x00000007, 0x00000007, 0x00040021, 0x00000009, 0x00000006,
    0x00000008, 0x00030021, 0x00000010, 0x00000007, 0x00040017, 0x00000013, 0x00000006, 0x00000004,
    0x00030021, 0x00000014, 0x00000013, 0x00040015, 0x00000017, 0x00000020, 0x00000000, 0x0006001e,
    0x00000018, 0x00000007, 0x00000017, 0x00000017, 0x00000006, 0x00040020, 0x00000019, 0x00000009,
    0x00000018, 0x0004003b, 0x00000019, 0x0000001a, 0x00000009, 0x00040015, 0x0000001b, 0x00000020,
    0x00000001, 0x0004002b, 0x0000001b, 0x0000001c, 0x00000001, 0x00040020, 0x0000001d, 0x00000009,
    0x00000017, 0x0004002b, 0x00000017, 0x00000020, 0x00000001, 0x00020014, 0x00000021, 0x00090019,
    0x00000025, 0x00000006, 0x00000001, 0x00000000, 0x00000000, 0x00000000, 0x00000001, 0x00000000,
    0x0003001b, 0x00000026, 0x00000025, 0x00040020, 0x00000027, 0x00000000, 0x00000026, 0x0004003b,
    0x00000027, 0x00000028, 0x00000000, 0x0004002b, 0x00000017, 0x0000002c, 0x00000000, 0x0004002b,
    0x00000017, 0x00000031, 0x00000002, 0x0004003b, 0x00000027, 0x00000035, 0x00000000, 0x0004002b,
    0x00000017, 0x0000003d, 0x00000003, 0x0004003b, 0x00000027, 0x00000041, 0x00000000, 0x0004002b,
    0x00000017, 0x00000049, 0x00000004, 0x0004003b, 0x00000027, 0x0000004d, 0x00000000, 0x0004002b,
    0x00000017, 0x00000055, 0x00000005, 0x0004003b, 0x00000027, 0x00000059, 0x00000000, 0x0004002b,
    0x00000017, 0x00000061, 0x00000006, 0x0004003b, 0x00000027, 0x00000065, 0x00000000, 0x0004002b,
    0x00000017, 0x0000006d, 0x00000007, 0x0004003b, 0x00000027, 0x00000071, 0x00000000, 0x0004002b,
    0x00000006, 0x00000077, 0x3f800000, 0x0004002b, 0x00000017, 0x0000007c, 0xffffffff, 0x00040020,
    0x00000081, 0x00000007, 0x00000006, 0x0004002b, 0x0000001b, 0x00000086, 0x00000002, 0x0004002b,
    0x0000001b, 0x00000093, 0x00000003, 0x00040020, 0x00000094, 0x00000009, 0x00000006, 0x0005002c,
    0x00000007, 0x00000098, 0x00000077, 0x00000077, 0x0004002b, 0x00000006, 0x0000009b, 0x3e4ccccd,
    0x0004002b, 0x00000006, 0x0000009d, 0x3f000000, 0x0004002b, 0x00000006, 0x0000009f, 0x00000000,
    0x0004002b, 0x00000006, 0x000000aa, 0x3ff33333, 0x0005002c, 0x00000007, 0x000000ac, 0x0000009f,
    0x00000077, 0x00040020, 0x000000b1, 0x00000007, 0x00000013, 0x0004002b, 0x00000006, 0x000000b3,
    0x40200000, 0x0007002c, 0x00000013, 0x000000b5, 0x0000009f, 0x0000009f, 0x00000077, 0x00000077,
    0x0004002b, 0x00000006, 0x000000b9, 0x40000000, 0x0007002c, 0x00000013, 0x000000ba, 0x00000077,
    0x000000b9, 0x00000077, 0x000000b9, 0x0004002b, 0x00000006, 0x000000d0, 0x40490fdb, 0x0004002b,
    0x00000006, 0x000000d3, 0xbf000000, 0x0004002b, 0x00000006, 0x000000ea, 0x3a83126f, 0x00040020,
    0x000000fd, 0x00000001, 0x00000007, 0x0004003b, 0x000000fd, 0x000000fe, 0x00000001, 0x0004003b,
    0x00000027, 0x00000106, 0x00000000, 0x0004003b, 0x000000fd, 0x00000108, 0x00000001, 0x00040020,
    0x00000141, 0x00000001, 0x00000013, 0x0004003b, 0x00000141, 0x00000142, 0x00000001, 0x00040020,
    0x00000145, 0x00000001, 0x00000006, 0x0004003b, 0x00000145, 0x00000146, 0x00000001, 0x00040020,
    0x0000014c, 0x00000003, 0x00000013, 0x0004003b, 0x0000014c, 0x0000014d, 0x00000003, 0x00040017,
    0x0000014e, 0x00000006, 0x00000003, 0x00050036, 0x00000002, 0x00000004, 0x00000000, 0x00000003,
    0x000200f8, 0x00000005, 0x0004003b, 0x000000b1, 0x0000013f, 0x00000007, 0x00040039, 0x00000013,
    0x00000140, 0x00000015, 0x0004003d, 0x00000013, 0x00000143, 0x00000142, 0x00050085, 0x00000013,
    0x00000144, 0x00000140, 0x00000143, 0x0003003e, 0x0000013f, 0x00000144, 0x0004003d, 0x00000006,
    0x00000147, 0x00000146, 0x00050041, 0x00000081, 0x00000148, 0x0000013f, 0x0000003d, 0x0004003d,
    0x00000006, 0x00000149, 0x00000148, 0x00050085, 0x00000006, 0x0000014a, 0x00000149, 0x00000147,
    0x00050041, 0x00000081, 0x0000014b, 0x0000013f, 0x0000003d, 0x0003003e, 0x0000014b, 0x0000014a,
    0x0004003d, 0x00000013, 0x0000014f, 0x0000013f, 0x0008004f, 0x0000014e, 0x00000150, 0x0000014f,
    0x0000014f, 0x00000000, 0x00000001, 0x00000002, 0x00050041, 0x00000081, 0x00000151, 0x0000013f,
    0x0000003d, 0x0004003d, 0x00000006, 0x00000152, 0x00000151, 0x0005008e, 0x0000014e, 0x00000153,
    0x00000150, 0x00000152, 0x00050041, 0x00000081, 0x00000154, 0x0000013f, 0x0000003d, 0x0004003d,
    0x00000006, 0x00000155, 0x00000154, 0x00050051, 0x00000006, 0x00000156, 0x00000153, 0x00000000,
    0x00050051, 0x00000006, 0x00000157, 0x00000153, 0x00000001, 0x00050051, 0x00000006, 0x00000158,
    0x00000153, 0x00000002, 0x00070050, 0x00000013, 0x00000159, 0x00000156, 0x00000157, 0x00000158,
    0x00000155, 0x0003003e, 0x0000014d, 0x00000159, 0x000100fd, 0x00010038, 0x00050036, 0x00000006,
    0x0000000b, 0x00000000, 0x00000009, 0x00030037, 0x00000008, 0x0000000a, 0x000200f8, 0x0000000c,
    0x00050041, 0x0000001d, 0x0000001e, 0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x0000001f,
    0x0000001e, 0x000500aa, 0x00000021, 0x00000022, 0x0000001f, 0x00000020, 0x000300f7, 0x00000024,
    0x00000000, 0x000400fa, 0x00000022, 0x00000023, 0x00000024, 0x000200f8, 0x00000023, 0x0004003d,
    0x00000026, 0x00000029, 0x00000028, 0x0004003d, 0x00000007, 0x0000002a, 0x0000000a, 0x00050057,
    0x00000013, 0x0000002b, 0x00000029, 0x0000002a, 0x00050051, 0x00000006, 0x0000002d, 0x0000002b,
    0x00000000, 0x000200fe, 0x0000002d, 0x000200f8, 0x00000024, 0x00050041, 0x0000001d, 0x0000002f,
    0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x00000030, 0x0000002f, 0x000500aa, 0x00000021,
    0x00000032, 0x00000030, 0x00000031, 0x000300f7, 0x00000034, 0x00000000, 0x000400fa, 0x00000032,
    0x00000033, 0x00000034, 0x000200f8, 0x00000033, 0x0004003d, 0x00000026, 0x00000036, 0x00000035,
    0x0004003d, 0x00000007, 0x00000037, 0x0000000a, 0x00050057, 0x00000013, 0x00000038, 0x00000036,
    0x00000037, 0x00050051, 0x00000006, 0x00000039, 0x00000038, 0x00000000, 0x000200fe, 0x00000039,
    0x000200f8, 0x00000034, 0x00050041, 0x0000001d, 0x0000003b, 0x0000001a, 0x0000001c, 0x0004003d,
    0x00000017, 0x0000003c, 0x0000003b, 0x000500aa, 0x00000021, 0x0000003e, 0x0000003c, 0x0000003d,
    0x000300f7, 0x00000040, 0x00000000, 0x000400fa, 0x0000003e, 0x0000003f, 0x00000040, 0x000200f8,
    0x0000003f, 0x0004003d, 0x00000026, 0x00000042, 0x00000041, 0x0004003d, 0x00000007, 0x00000043,
    0x0000000a, 0x00050057, 0x00000013, 0x00000044, 0x00000042, 0x00000043, 0x00050051, 0x00000006,
    0x00000045, 0x00000044, 0x00000000, 0x000200fe, 0x00000045, 0x000200f8, 0x00000040, 0x00050041,
    0x0000001d, 0x00000047, 0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x00000048, 0x00000047,
    0x000500aa, 0x00000021, 0x0000004a, 0x00000048, 0x00000049, 0x000300f7, 0x0000004c, 0x00000000,
    0x000400fa, 0x0000004a, 0x0000004b, 0x0000004c, 0x000200f8, 0x0000004b, 0x0004003d, 0x00000026,
    0x0000004e, 0x0000004d, 0x0004003d, 0x00000007, 0x0000004f, 0x0000000a, 0x00050057, 0x00000013,
    0x00000050, 0x0000004e, 0x0000004f, 0x00050051, 0x00000006, 0x00000051, 0x00000050, 0x00000000,
    0x000200fe, 0x00000051, 0x000200f8, 0x0000004c, 0x00050041, 0x0000001d, 0x00000053, 0x0000001a,
    0x0000001c, 0x0004003d, 0x00000017, 0x00000054, 0x00000053, 0x000500aa, 0x00000021, 0x00000056,
    0x00000054, 0x00000055, 0x000300f7, 0x00000058, 0x00000000, 0x000400fa, 0x00000056, 0x00000057,
    0x00000058, 0x000200f8, 0x00000057, 0x0004003d, 0x00000026, 0x0000005a, 0x00000059, 0x0004003d,
    0x00000007, 0x0000005b, 0x0000000a, 0x00050057, 0x00000013, 0x0000005c, 0x0000005a, 0x0000005b,
    0x00050051, 0x00000006, 0x0000005d, 0x0000005c, 0x00000000, 0x000200fe, 0x0000005d, 0x000200f8,
    0x00000058, 0x00050041, 0x0000001d, 0x0000005f, 0x0000001a, 0x0000001c, 0x0004003d, 0x00000017,
    0x00000060, 0x0000005f, 0x000500aa, 0x00000021, 0x00000062, 0x00000060, 0x00000061, 0x000300f7,
    0x00000064, 0x00000000, 0x000400fa, 0x00000062, 0x00000063, 0x00000064, 0x000200f8, 0x00000063,
    0x0004003d, 0x00000026, 0x00000066, 0x00000065, 0x0004003d, 0x00000007, 0x00000067, 0x0000000a,
    0x00050057, 0x00000013, 0x00000068, 0x00000066, 0x00000067, 0x00050051, 0x00000006, 0x00000069,
    0x00000068, 0x00000000, 0x000200fe, 0x00000069, 0x000200f8, 0x00000064, 0x00050041, 0x0000001d,
    0x0000006b, 0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x0000006c, 0x0000006b, 0x000500aa,
    0x00000021, 0x0000006e, 0x0000006c, 0x0000006d, 0x000300f7, 0x00000070, 0x00000000, 0x000400fa,
    0x0000006e, 0x0000006f, 0x00000070, 0x000200f8, 0x0000006f, 0x0004003d, 0x00000026, 0x00000072,
    0x00000071, 0x0004003d, 0x00000007, 0x00000073, 0x0000000a, 0x00050057, 0x00000013, 0x00000074,
    0x00000072, 0x00000073, 0x00050051, 0x00000006, 0x00000075, 0x00000074, 0x00000000, 0x000200fe,
    0x00000075, 0x000200f8, 0x00000070, 0x000200fe, 0x00000077, 0x00010038, 0x00050036, 0x00000006,
    0x0000000e, 0x00000000, 0x00000009, 0x00030037, 0x00000008, 0x0000000d, 0x000200f8, 0x0000000f,
    0x0004003b, 0x00000081, 0x00000082, 0x00000007, 0x0004003b, 0x00000008, 0x00000083, 0x00000007,
    0x00050041, 0x0000001d, 0x0000007a, 0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x0000007b,
    0x0000007a, 0x000500aa, 0x00000021, 0x0000007d, 0x0000007b, 0x0000007c, 0x000300f7, 0x0000007f,
    0x00000000, 0x000400fa, 0x0000007d, 0x0000007e, 0x0000007f, 0x000200f8, 0x0000007e, 0x000200fe,
    0x00000077, 0x000200f8, 0x0000007f, 0x0004003d, 0x00000007, 0x00000084, 0x0000000d, 0x0003003e,
    0x00000083, 0x00000084, 0x00050039, 0x00000006, 0x00000085, 0x0000000b, 0x00000083, 0x0003003e,
    0x00000082, 0x00000085, 0x00050041, 0x0000001d, 0x00000087, 0x0000001a, 0x00000086, 0x0004003d,
    0x00000017, 0x00000088, 0x00000087, 0x000500aa, 0x00000021, 0x00000089, 0x00000088, 0x00000020,
    0x000300f7, 0x0000008b, 0x00000000, 0x000400fa, 0x00000089, 0x0000008a, 0x0000008b, 0x000200f8,
    0x0000008a, 0x0004003d, 0x00000006, 0x0000008c, 0x00000082, 0x00050083, 0x00000006, 0x0000008d,
    0x00000077, 0x0000008c, 0x000200fe, 0x0000008d, 0x000200f8, 0x0000008b, 0x0004003d, 0x00000006,
    0x0000008f, 0x00000082, 0x000200fe, 0x0000008f, 0x00010038, 0x00050036, 0x00000007, 0x00000011,
    0x00000000, 0x00000010, 0x000200f8, 0x00000012, 0x0004003b, 0x00000081, 0x00000092, 0x00000007,
    0x0004003b, 0x00000008, 0x00000097, 0x00000007, 0x0004003b, 0x00000081, 0x00000099, 0x00000007,
    0x0004003b, 0x00000081, 0x0000009a, 0x00000007, 0x0004003b, 0x00000081, 0x0000009c, 0x00000007,
    0x0004003b, 0x00000081, 0x0000009e, 0x00000007, 0x0004003b, 0x00000081, 0x000000a0, 0x00000007,
    0x0004003b, 0x00000081, 0x000000a6, 0x00000007, 0x0004003b, 0x00000008, 0x000000a9, 0x00000007,
    0x0004003b, 0x000000b1, 0x000000b2, 0x00000007, 0x0004003b, 0x00000008, 0x000000bd, 0x00000007,
    0x0004003b, 0x00000008, 0x000000c3, 0x00000007, 0x0004003b, 0x00000008, 0x000000c9, 0x00000007,
    0x00050041, 0x00000094, 0x00000095, 0x0000001a, 0x00000093, 0x0004003d, 0x00000006, 0x00000096,
    0x00000095, 0x0003003e, 0x00000092, 0x00000096, 0x0003003e, 0x00000097, 0x00000098, 0x0003003e,
    0x00000099, 0x00000077, 0x0003003e, 0x0000009a, 0x0000009b, 0x0003003e, 0x0000009c, 0x0000009d,
    0x0003003e, 0x0000009e, 0x0000009f, 0x0004003d, 0x00000006, 0x000000a1, 0x00000092, 0x0004003d,
    0x00000006, 0x000000a2, 0x00000099, 0x00050085, 0x00000006, 0x000000a3, 0x000000a1, 0x000000a2,
    0x0004003d, 0x00000006, 0x000000a4, 0x0000009e, 0x00050081, 0x00000006, 0x000000a5, 0x000000a3,
    0x000000a4, 0x0003003e, 0x000000a0, 0x000000a5, 0x0004003d, 0x00000006, 0x000000a7, 0x000000a0,
    0x0006000c, 0x00000006, 0x000000a8, 0x00000001, 0x00000008, 0x000000a7, 0x0003003e, 0x000000a6,
    0x000000a8, 0x0004003d, 0x00000006, 0x000000ab, 0x000000a6, 0x00050050, 0x00000007, 0x000000ad,
    0x000000ab, 0x000000ab, 0x00050081, 0x00000007, 0x000000ae, 0x000000ad, 0x000000ac, 0x0005008e,
    0x00000007, 0x000000af, 0x000000ae, 0x000000aa, 0x0006000c, 0x00000007, 0x000000b0, 0x00000001,
    0x0000000d, 0x000000af, 0x0003003e, 0x000000a9, 0x000000b0, 0x0004003d, 0x00000006, 0x000000b4,
    0x000000a6, 0x00070050, 0x00000013, 0x000000b6, 0x000000b4, 0x000000b4, 0x000000b4, 0x000000b4,
    0x00050081, 0x00000013, 0x000000b7, 0x000000b6, 0x000000b5, 0x0005008e, 0x00000013, 0x000000b8,
    0x000000b7, 0x000000b3, 0x00050081, 0x00000013, 0x000000bb, 0x000000b8, 0x000000ba, 0x0006000c,
    0x00000013, 0x000000bc, 0x00000001, 0x0000000d, 0x000000bb, 0x0003003e, 0x000000b2, 0x000000bc,
    0x0004003d, 0x00000007, 0x000000be, 0x000000a9, 0x0007004f, 0x00000007, 0x000000bf, 0x000000be,
    0x000000be, 0x00000000, 0x00000000, 0x0004003d, 0x00000013, 0x000000c0, 0x000000b2, 0x0007004f,
    0x00000007, 0x000000c1, 0x000000c0, 0x000000c0, 0x00000000, 0x00000001, 0x00050081, 0x00000007,
    0x000000c2, 0x000000bf, 0x000000c1, 0x0003003e, 0x000000bd, 0x000000c2, 0x0004003d, 0x00000007,
    0x000000c4, 0x000000a9, 0x0007004f, 0x00000007, 0x000000c5, 0x000000c4, 0x000000c4, 0x00000001,
    0x00000001, 0x0004003d, 0x00000013, 0x000000c6, 0x000000b2, 0x0007004f, 0x00000007, 0x000000c7,
    0x000000c6, 0x000000c6, 0x00000002, 0x00000003, 0x00050081, 0x00000007, 0x000000c8, 0x000000c5,
    0x000000c7, 0x0003003e, 0x000000c3, 0x000000c8, 0x0004003d, 0x00000007, 0x000000ca, 0x000000bd,
    0x0004003d, 0x00000007, 0x000000cb, 0x000000c3, 0x0004003d, 0x00000006, 0x000000cc, 0x0000009a,
    0x00050083, 0x00000006, 0x000000cd, 0x00000077, 0x000000cc, 0x0004003d, 0x00000006, 0x000000ce,
    0x000000a0, 0x0006000c, 0x00000006, 0x000000cf, 0x00000001, 0x0000000a, 0x000000ce, 0x00050085,
    0x00000006, 0x000000d1, 0x000000cf, 0x000000d0, 0x0006000c, 0x00000006, 0x000000d2, 0x00000001,
    0x0000000e, 0x000000d1, 0x00050085, 0x00000006, 0x000000d4, 0x000000d2, 0x000000d3, 0x00050081,
    0x00000006, 0x000000d5, 0x000000d4, 0x0000009d, 0x0008000c, 0x00000006, 0x000000d6, 0x00000001,
    0x00000031, 0x000000cd, 0x00000077, 0x000000d5, 0x00050050, 0x00000007, 0x000000d7, 0x000000d6,
    0x000000d6, 0x0008000c, 0x00000007, 0x000000d8, 0x00000001, 0x0000002e, 0x000000ca, 0x000000cb,
    0x000000d7, 0x0003003e, 0x000000c9, 0x000000d8, 0x0004003d, 0x00000006, 0x000000d9, 0x000000a0,
    0x0006000c, 0x00000006, 0x000000da, 0x00000001, 0x0000000d, 0x000000d9, 0x0004003d, 0x00000006,
    0x000000db, 0x0000009c, 0x00050085, 0x00000006, 0x000000dc, 0x000000da, 0x000000db, 0x00050041,
    0x00000081, 0x000000dd, 0x000000c9, 0x0000002c, 0x0004003d, 0x00000006, 0x000000de, 0x000000dd,
    0x00050081, 0x00000006, 0x000000df, 0x000000de, 0x000000dc, 0x00050041, 0x00000081, 0x000000e0,
    0x000000c9, 0x0000002c, 0x0003003e, 0x000000e0, 0x000000df, 0x0004003d, 0x00000006, 0x000000e1,
    0x000000a0, 0x0006000c, 0x00000006, 0x000000e2, 0x00000001, 0x0000000e, 0x000000e1, 0x0004003d,
    0x00000006, 0x000000e3, 0x0000009c, 0x00050085, 0x00000006, 0x000000e4, 0x000000e2, 0x000000e3,
    0x00050041, 0x00000081, 0x000000e5, 0x000000c9, 0x00000020, 0x0004003d, 0x00000006, 0x000000e6,
    0x000000e5, 0x00050081, 0x00000006, 0x000000e7, 0x000000e6, 0x000000e4, 0x00050041, 0x00000081,
    0x000000e8, 0x000000c9, 0x00000020, 0x0003003e, 0x000000e8, 0x000000e7, 0x0004003d, 0x00000007,
    0x000000e9, 0x00000097, 0x0005008e, 0x00000007, 0x000000eb, 0x000000e9, 0x000000ea, 0x0004003d,
    0x00000007, 0x000000ec, 0x000000c9, 0x00050085, 0x00000007, 0x000000ed, 0x000000ec, 0x000000eb,
    0x0003003e, 0x000000c9, 0x000000ed, 0x0004003d, 0x00000007, 0x000000ee, 0x000000c9, 0x000200fe,
    0x000000ee, 0x00010038, 0x00050036, 0x00000013, 0x00000015, 0x00000000, 0x00000014, 0x000200f8,
    0x00000016, 0x0004003b, 0x00000081, 0x000000fc, 0x00000007, 0x0004003b, 0x00000008, 0x000000ff,
    0x00000007, 0x0004003b, 0x00000008, 0x00000102, 0x00000007, 0x0004003b, 0x00000081, 0x00000119,
    0x00000007, 0x0004003b, 0x00000008, 0x0000011a, 0x00000007, 0x0004003b, 0x00000008, 0x0000011d,
    0x00000007, 0x0004003b, 0x000000b1, 0x00000121, 0x00000007, 0x0004003b, 0x000000b1, 0x00000131,
    0x00000007, 0x0004003b, 0x00000008, 0x00000135, 0x00000007, 0x00050041, 0x0000001d, 0x000000f1,
    0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x000000f2, 0x000000f1, 0x000500ab, 0x00000021,
    0x000000f3, 0x000000f2, 0x0000007c, 0x000300f7, 0x000000f5, 0x00000000, 0x000400fa, 0x000000f3,
    0x000000f4, 0x000000f5, 0x000200f8, 0x000000f4, 0x00050041, 0x0000001d, 0x000000f6, 0x0000001a,
    0x00000086, 0x0004003d, 0x00000017, 0x000000f7, 0x000000f6, 0x000500aa, 0x00000021, 0x000000f8,
    0x000000f7, 0x00000031, 0x000200f9, 0x000000f5, 0x000200f8, 0x000000f5, 0x000700f5, 0x00000021,
    0x000000f9, 0x000000f3, 0x00000016, 0x000000f8, 0x000000f4, 0x000300f7, 0x000000fb, 0x00000000,
    0x000400fa, 0x000000f9, 0x000000fa, 0x000000fb, 0x000200f8, 0x000000fa, 0x0004003d, 0x00000007,
    0x00000100, 0x000000fe, 0x0003003e, 0x000000ff, 0x00000100, 0x00050039, 0x00000006, 0x00000101,
    0x0000000b, 0x000000ff, 0x0003003e, 0x000000fc, 0x00000101, 0x00040039, 0x00000007, 0x00000103,
    0x00000011, 0x0004003d, 0x00000006, 0x00000104, 0x000000fc, 0x0005008e, 0x00000007, 0x00000105,
    0x00000103, 0x00000104, 0x0003003e, 0x00000102, 0x00000105, 0x0004003d, 0x00000026, 0x00000107,
    0x00000106, 0x0004003d, 0x00000007, 0x00000109, 0x00000108, 0x0004003d, 0x00000007, 0x0000010a,
    0x00000102, 0x00050081, 0x00000007, 0x0000010b, 0x00000109, 0x0000010a, 0x00050057, 0x00000013,
    0x0000010c, 0x00000107, 0x0000010b, 0x000200fe, 0x0000010c, 0x000200f8, 0x000000fb, 0x00050041,
    0x0000001d, 0x0000010e, 0x0000001a, 0x0000001c, 0x0004003d, 0x00000017, 0x0000010f, 0x0000010e,
    0x000500ab, 0x00000021, 0x00000110, 0x0000010f, 0x0000007c, 0x000300f7, 0x00000112, 0x00000000,
    0x000400fa, 0x00000110, 0x00000111, 0x00000112, 0x000200f8, 0x00000111, 0x00050041, 0x0000001d,
    0x00000113, 0x0000001a, 0x00000086, 0x0004003d, 0x00000017, 0x00000114, 0x00000113, 0x000500aa,
    0x00000021, 0x00000115, 0x00000114, 0x00000049, 0x000200f9, 0x00000112, 0x000200f8, 0x00000112,
    0x000700f5, 0x00000021, 0x00000116, 0x00000110, 0x000000fb, 0x00000115, 0x00000111, 0x000300f7,
    0x00000118, 0x00000000, 0x000400fa, 0x00000116, 0x00000117, 0x00000118, 0x000200f8, 0x00000117,
    0x0004003d, 0x00000007, 0x0000011b, 0x000000fe, 0x0003003e, 0x0000011a, 0x0000011b, 0x00050039,
    0x00000006, 0x0000011c, 0x0000000b, 0x0000011a, 0x0003003e, 0x00000119, 0x0000011c, 0x00040039,
    0x00000007, 0x0000011e, 0x00000011, 0x0004003d, 0x00000006, 0x0000011f, 0x00000119, 0x0005008e,
    0x00000007, 0x00000120, 0x0000011e, 0x0000011f, 0x0003003e, 0x0000011d, 0x00000120, 0x0004003d,
    0x00000026, 0x00000122, 0x00000106, 0x0004003d, 0x00000007, 0x00000123, 0x00000108, 0x0004003d,
    0x00000007, 0x00000124, 0x0000011d, 0x00050081, 0x00000007, 0x00000125, 0x00000123, 0x00000124,
    0x00050057, 0x00000013, 0x00000126, 0x00000122, 0x00000125, 0x0003003e, 0x00000121, 0x00000126,
    0x0004003d, 0x00000026, 0x00000127, 0x00000035, 0x0004003d, 0x00000007, 0x00000128, 0x000000fe,
    0x00050057, 0x00000013, 0x00000129, 0x00000127, 0x00000128, 0x00050051, 0x00000006, 0x0000012a,
    0x00000129, 0x00000000, 0x00050041, 0x00000081, 0x0000012b, 0x00000121, 0x0000003d, 0x0004003d,
    0x00000006, 0x0000012c, 0x0000012b, 0x00050085, 0x00000006, 0x0000012d, 0x0000012c, 0x0000012a,
    0x00050041, 0x00000081, 0x0000012e, 0x00000121, 0x0000003d, 0x0003003e, 0x0000012e, 0x0000012d,
    0x0004003d, 0x00000013, 0x0000012f, 0x00000121, 0x000200fe, 0x0000012f, 0x000200f8, 0x00000118,
    0x0004003d, 0x00000026, 0x00000132, 0x00000106, 0x0004003d, 0x00000007, 0x00000133, 0x00000108,
    0x00050057, 0x00000013, 0x00000134, 0x00000132, 0x00000133, 0x0003003e, 0x00000131, 0x00000134,
    0x0004003d, 0x00000007, 0x00000136, 0x000000fe, 0x0003003e, 0x00000135, 0x00000136, 0x00050039,
    0x00000006, 0x00000137, 0x0000000e, 0x00000135, 0x00050041, 0x00000081, 0x00000138, 0x00000131,
    0x0000003d, 0x0004003d, 0x00000006, 0x00000139, 0x00000138, 0x00050085, 0x00000006, 0x0000013a,
    0x00000139, 0x00000137, 0x00050041, 0x00000081, 0x0000013b, 0x00000131, 0x0000003d, 0x0003003e,
    0x0000013b, 0x0000013a, 0x0004003d, 0x00000013, 0x0000013c, 0x00000131, 0x000200fe, 0x0000013c,
    0x00010038,
];

#[cfg(test)]
mod tests {
    use super::*;

    fn blend_state(
        mode: SceneBlendMode,
    ) -> super::super::present::NativeVulkanVulkanaliaSceneBlendState {
        super::super::present::NativeVulkanVulkanaliaSceneBlendState::from_mode(mode)
    }

    fn sampled_image_material(
        blend_mode: SceneBlendMode,
    ) -> super::super::present::NativeVulkanVulkanaliaSceneSampledImageMaterial {
        super::super::present::NativeVulkanVulkanaliaSceneSampledImageMaterial::sampled_image(
            blend_mode,
            None,
            SceneRenderAlphaTextureMode::Multiply,
            1,
        )
    }

    fn texture_slot_bindings(
        resources: &[u32],
    ) -> Vec<super::super::present::NativeVulkanVulkanaliaSceneTextureSlotResourceBinding> {
        resources
            .iter()
            .copied()
            .enumerate()
            .map(|(slot, resource_index)| {
                super::super::present::NativeVulkanVulkanaliaSceneTextureSlotResourceBinding {
                    slot: slot.min(u32::MAX as usize) as u32,
                    resource_index,
                }
            })
            .collect()
    }

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
            vec![
                "scene-solid-quad-alpha-blend",
                "scene-solid-quad-additive-blend",
                "scene-solid-quad-multiply-blend",
                "scene-solid-quad-screen-blend",
                "scene-solid-quad-max-blend"
            ]
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
        input.sampled_image_vertex_buffer_bytes = 176;
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
            vec![
                "scene-sampled-image-alpha-blend",
                "scene-sampled-image-additive-blend",
                "scene-sampled-image-multiply-blend",
                "scene-sampled-image-screen-blend",
                "scene-sampled-image-max-blend"
            ]
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
            vec![
                "scene-sampled-image-alpha-blend",
                "scene-sampled-image-additive-blend",
                "scene-sampled-image-multiply-blend",
                "scene-sampled-image-screen-blend",
                "scene-sampled-image-max-blend"
            ]
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
                "scene-solid-quad-additive-blend",
                "scene-solid-quad-multiply-blend",
                "scene-solid-quad-screen-blend",
                "scene-solid-quad-max-blend",
                "scene-sampled-image-alpha-blend",
                "scene-sampled-image-additive-blend",
                "scene-sampled-image-multiply-blend",
                "scene-sampled-image-screen-blend",
                "scene-sampled-image-max-blend"
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
        assert_eq!(snapshot.vertex_input_attribute_count, 5);
        assert_eq!(
            snapshot.vertex_stride_bytes,
            SCENE_FULL_SAMPLED_IMAGE_VERTEX_STRIDE_BYTES
        );
        assert_eq!(snapshot.vertex_uv_format, "R32G32_SFLOAT");
        assert_eq!(snapshot.vertex_effect_uv_format, "R32G32_SFLOAT");
        assert_eq!(snapshot.vertex_opacity_format, "R32_SFLOAT");
        assert_eq!(snapshot.vertex_tint_format, "R32G32B32A32_SFLOAT");
        assert_eq!(
            snapshot.sampled_image_model,
            "retained native sampled image -> VK_EXT_descriptor_heap constant-offset mapping -> fragment shader"
        );
        assert!(snapshot.uses_pipeline_rendering_create_info);
        assert!(snapshot.uses_dynamic_rendering);
        assert!(snapshot.uses_synchronization2);
        assert!(snapshot.uses_submit2);
        assert_eq!(snapshot.descriptor_set_count, 0);
        assert_eq!(snapshot.descriptor_model, "VK_EXT_descriptor_heap");
        assert!(snapshot.descriptor_heap_mapping_enabled);
        assert!(snapshot.descriptor_heap_pipeline_flag_enabled);
        assert_eq!(
            snapshot.blend_model,
            "sampled rgba with opacity; alpha/additive/multiply/screen/max blend pipeline selected per draw command"
        );
        assert!(snapshot.descriptor_set_layout_create_flags.is_empty());
        assert!(!snapshot.uses_push_descriptor_fast_path);
    }

    #[test]
    fn scene_blend_attachments_cover_alpha_additive_multiply_screen_and_max_modes() {
        let alpha = native_vulkan_vulkanalia_scene_color_attachment(SceneBlendMode::Alpha);
        let additive = native_vulkan_vulkanalia_scene_color_attachment(SceneBlendMode::Additive);
        let multiply = native_vulkan_vulkanalia_scene_color_attachment(SceneBlendMode::Multiply);
        let screen = native_vulkan_vulkanalia_scene_color_attachment(SceneBlendMode::Screen);
        let max = native_vulkan_vulkanalia_scene_color_attachment(SceneBlendMode::Max);

        assert_eq!(alpha.src_color_blend_factor, vk::BlendFactor::SRC_ALPHA);
        assert_eq!(
            alpha.dst_color_blend_factor,
            vk::BlendFactor::ONE_MINUS_SRC_ALPHA
        );
        assert_eq!(alpha.color_blend_op, vk::BlendOp::ADD);
        assert_eq!(alpha.src_alpha_blend_factor, vk::BlendFactor::SRC_ALPHA);
        assert_eq!(
            alpha.dst_alpha_blend_factor,
            vk::BlendFactor::ONE_MINUS_SRC_ALPHA
        );
        assert_eq!(additive.src_color_blend_factor, vk::BlendFactor::SRC_ALPHA);
        assert_eq!(additive.dst_color_blend_factor, vk::BlendFactor::ONE);
        assert_eq!(additive.color_blend_op, vk::BlendOp::ADD);
        assert_eq!(multiply.src_color_blend_factor, vk::BlendFactor::DST_COLOR);
        assert_eq!(
            multiply.dst_color_blend_factor,
            vk::BlendFactor::ONE_MINUS_SRC_ALPHA
        );
        assert_eq!(multiply.color_blend_op, vk::BlendOp::ADD);
        assert_eq!(
            screen.src_color_blend_factor,
            vk::BlendFactor::ONE_MINUS_DST_COLOR
        );
        assert_eq!(screen.dst_color_blend_factor, vk::BlendFactor::ONE);
        assert_eq!(screen.color_blend_op, vk::BlendOp::ADD);
        assert_eq!(max.src_color_blend_factor, vk::BlendFactor::ONE);
        assert_eq!(max.dst_color_blend_factor, vk::BlendFactor::ONE);
        assert_eq!(max.color_blend_op, vk::BlendOp::MAX);
        assert_eq!(
            max.dst_alpha_blend_factor,
            vk::BlendFactor::ONE_MINUS_SRC_ALPHA
        );
        assert_eq!(max.alpha_blend_op, vk::BlendOp::ADD);
    }

    #[test]
    fn non_alpha_blend_modes_use_premultiplied_fragment_shader() {
        let straight = vk::ShaderModule::from_raw(1);
        let premultiplied = vk::ShaderModule::from_raw(2);

        assert_eq!(
            native_vulkan_vulkanalia_scene_fragment_module_for_blend(
                SceneBlendMode::Alpha,
                straight,
                premultiplied
            ),
            straight
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_fragment_module_for_blend(
                SceneBlendMode::Additive,
                straight,
                premultiplied
            ),
            straight
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_fragment_module_for_blend(
                SceneBlendMode::Multiply,
                straight,
                premultiplied
            ),
            premultiplied
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_fragment_module_for_blend(
                SceneBlendMode::Screen,
                straight,
                premultiplied
            ),
            premultiplied
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_fragment_module_for_blend(
                SceneBlendMode::Max,
                straight,
                premultiplied
            ),
            premultiplied
        );
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
            last_layer_index: 2,
            blend: blend_state(SceneBlendMode::Alpha),
            first_index: 0,
            index_count: 6,
        }];
        let sampled_commands = [
            VulkanaliaSceneSampledImageDrawCommand {
                layer_index: 1,
                last_layer_index: 1,
                material: sampled_image_material(SceneBlendMode::Alpha),
                descriptor_binding: VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap {
                    descriptor_group_base_index: 0,
                    texture_slot_bindings: texture_slot_bindings(&[0]),
                },
                render_target: VulkanaliaSceneSampledImageRenderTarget::Swapchain,
                first_index: 0,
                index_count: 6,
            },
            VulkanaliaSceneSampledImageDrawCommand {
                layer_index: 3,
                last_layer_index: 3,
                material: sampled_image_material(SceneBlendMode::Alpha),
                descriptor_binding: VulkanaliaSceneSampledImageDescriptorBinding::DescriptorHeap {
                    descriptor_group_base_index: SCENE_SAMPLED_IMAGE_TEXTURE_SLOT_BINDING_COUNT
                        as u32,
                    texture_slot_bindings: texture_slot_bindings(&[1]),
                },
                render_target: VulkanaliaSceneSampledImageRenderTarget::Swapchain,
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
            NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SOLID_QUAD_PREMULTIPLIED_FRAGMENT_SPIRV[0],
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
            NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_PREMULTIPLIED_FRAGMENT_SPIRV[0],
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
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SOLID_QUAD_PREMULTIPLIED_FRAGMENT_SPIRV
            ),
            656
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_VERTEX_SPIRV
            ),
            2024
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_FRAGMENT_SPIRV
            ),
            7776
        );
        assert_eq!(
            native_vulkan_vulkanalia_scene_shader_code_size_bytes(
                &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_PREMULTIPLIED_FRAGMENT_SPIRV
            ),
            8004
        );
    }

    #[test]
    fn sampled_image_fragment_shader_samples_alpha_mask_from_effect_uv() {
        assert!(spirv_function_argument_loads_named_input(
            &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_FRAGMENT_SPIRV,
            "alpha_mask",
            "v_effect_uv"
        ));
        assert!(spirv_function_argument_loads_named_input(
            &NATIVE_VULKAN_VULKANALIA_SCENE_FULL_SAMPLED_IMAGE_PREMULTIPLIED_FRAGMENT_SPIRV,
            "alpha_mask",
            "v_effect_uv"
        ));
    }

    fn spirv_function_argument_loads_named_input(
        words: &[u32],
        function_name_prefix: &str,
        input_name: &str,
    ) -> bool {
        let Some(function_id) = spirv_named_id(words, function_name_prefix, true) else {
            return false;
        };
        let Some(input_id) = spirv_named_id(words, input_name, false) else {
            return false;
        };
        let offsets = spirv_instruction_offsets(words);
        for (call_position, offset) in offsets.iter().enumerate() {
            let word_count = spirv_word_count(words[*offset]) as usize;
            if spirv_opcode(words[*offset]) != 57
                || word_count < 5
                || words[*offset + 3] != function_id
            {
                continue;
            }
            let argument_id = words[*offset + 4];
            let Some(loaded_id) =
                spirv_latest_store_object(words, &offsets[..call_position], argument_id)
            else {
                continue;
            };
            if spirv_loads_input_before(words, &offsets[..call_position], loaded_id, input_id) {
                return true;
            }
        }
        false
    }

    fn spirv_latest_store_object(words: &[u32], offsets: &[usize], pointer_id: u32) -> Option<u32> {
        offsets.iter().rev().find_map(|offset| {
            (spirv_opcode(words[*offset]) == 62
                && spirv_word_count(words[*offset]) == 3
                && words[*offset + 1] == pointer_id)
                .then_some(words[*offset + 2])
        })
    }

    fn spirv_loads_input_before(
        words: &[u32],
        offsets: &[usize],
        loaded_id: u32,
        input_id: u32,
    ) -> bool {
        offsets.iter().rev().any(|offset| {
            spirv_opcode(words[*offset]) == 61
                && spirv_word_count(words[*offset]) >= 4
                && words[*offset + 2] == loaded_id
                && words[*offset + 3] == input_id
        })
    }

    fn spirv_named_id(words: &[u32], name: &str, prefix: bool) -> Option<u32> {
        for offset in spirv_instruction_offsets(words) {
            let word_count = spirv_word_count(words[offset]) as usize;
            if spirv_opcode(words[offset]) != 5 || word_count < 3 {
                continue;
            }
            let decoded = spirv_string(&words[offset + 2..offset + word_count]);
            if decoded == name || (prefix && decoded.starts_with(name)) {
                return Some(words[offset + 1]);
            }
        }
        None
    }

    fn spirv_instruction_offsets(words: &[u32]) -> Vec<usize> {
        let mut offsets = Vec::new();
        let mut offset = 5usize;
        while offset < words.len() {
            let word_count = spirv_word_count(words[offset]) as usize;
            if word_count == 0 || offset.saturating_add(word_count) > words.len() {
                break;
            }
            offsets.push(offset);
            offset += word_count;
        }
        offsets
    }

    fn spirv_word_count(word: u32) -> u16 {
        (word >> 16) as u16
    }

    fn spirv_opcode(word: u32) -> u16 {
        (word & 0xffff) as u16
    }

    fn spirv_string(words: &[u32]) -> String {
        let mut bytes = Vec::with_capacity(words.len().saturating_mul(4));
        for word in words {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        let end = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len());
        String::from_utf8_lossy(&bytes[..end]).into_owned()
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
