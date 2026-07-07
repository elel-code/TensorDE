//! Scene mesh graphics pipeline factory.
//!
//! References:
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `reverse-engineered/docs/mdl-format.md`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/servers/rendering/renderer_rd/renderer_canvas_render_rd.h`
//! - `references/godot/servers/rendering/renderer_rd/pipeline_hash_map_rd.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SCENE_GPU_MESH_VERTEX_BYTES, SceneBlendContract, SceneCullMode, SceneDepthTest,
    WeShaderInterface,
};
use crate::renderer::native_vulkan::vulkan::{
    NativeVulkanDescriptorHeapShaderBindingMapping,
    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_combined_image_sampler_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_sampled_image_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping_info,
};

use super::pipeline::{
    NativeVulkanScenePipelineCacheKey, NativeVulkanScenePipelineResources,
    NativeVulkanScenePipelineVertexLayout,
};
use super::shader_module::{
    native_vulkan_create_scene_shader_module, native_vulkan_validate_scene_spirv,
};

#[derive(Debug, Clone, Copy)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMeshPipelineShaders<'a> {
    pub vertex_spirv: &'a [u32],
    pub fragment_spirv: &'a [u32],
}

#[derive(Debug, Clone, Copy)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMeshPipelineLayoutSpec<'a> {
    pub draw_resource_heap_plan: &'a NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    pub layer_alpha_mask_resource_heap_plan:
        Option<&'a NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneMeshPipelineCreatePlan {
    pub shader: String,
    pub target_format: String,
    pub vertex_stride_bytes: u32,
    pub vertex_attributes: Vec<&'static str>,
    pub dynamic_states: [&'static str; 2],
    pub shader_resource_mappings: Vec<String>,
    pub blend: &'static str,
    pub depth_test: &'static str,
    pub depth_write: bool,
    pub cull_mode: &'static str,
    pub alpha_write: &'static str,
    pub dynamic_rendering_scope: &'static str,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_mesh_pipeline_create_plan(
    key: &NativeVulkanScenePipelineCacheKey,
) -> Result<NativeVulkanSceneMeshPipelineCreatePlan, String> {
    validate_scene_mesh_pipeline_key(key)?;
    Ok(NativeVulkanSceneMeshPipelineCreatePlan {
        shader: key.shader.clone(),
        target_format: format!("{:?}", key.target_format),
        vertex_stride_bytes: scene_pipeline_vertex_stride_bytes(key.vertex_layout),
        vertex_attributes: scene_pipeline_vertex_attribute_labels(key.vertex_layout),
        dynamic_states: ["viewport", "scissor"],
        shader_resource_mappings: scene_mesh_shader_resource_mapping_labels(key),
        blend: scene_pipeline_blend_label(key.blend),
        depth_test: scene_depth_test_label(key.render_state.depth_test),
        depth_write: key.render_state.depth_write,
        cull_mode: scene_cull_mode_label(key.render_state.cull_mode),
        alpha_write: scene_alpha_write_label(key.render_state.alpha_write),
        dynamic_rendering_scope: "dynamic-rendering-no-render-pass",
    })
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_create_scene_mesh_pipeline_resources(
    device: &Device,
    key: &NativeVulkanScenePipelineCacheKey,
    shaders: NativeVulkanSceneMeshPipelineShaders<'_>,
    layout: NativeVulkanSceneMeshPipelineLayoutSpec<'_>,
) -> Result<NativeVulkanScenePipelineResources, String> {
    validate_scene_mesh_pipeline_key(key)?;
    native_vulkan_validate_scene_spirv(shaders.vertex_spirv, "scene mesh vertex")?;
    native_vulkan_validate_scene_spirv(shaders.fragment_spirv, "scene mesh fragment")?;
    validate_scene_mesh_descriptor_heap_pipeline_layout(layout, key)?;

    let result = (|| -> Result<NativeVulkanScenePipelineResources, String> {
        let vertex_module = native_vulkan_create_scene_shader_module(
            device,
            shaders.vertex_spirv,
            "scene mesh vertex",
        )?;
        let result = (|| -> Result<NativeVulkanScenePipelineResources, String> {
            let fragment_module = native_vulkan_create_scene_shader_module(
                device,
                shaders.fragment_spirv,
                "scene mesh fragment",
            )?;
            let result = (|| -> Result<NativeVulkanScenePipelineResources, String> {
                let shader_entry = b"main\0";
                let descriptor_heap_mappings =
                    scene_pipeline_descriptor_heap_mappings(layout, key)?;
                let mut descriptor_heap_mapping_info =
                    native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping_info(
                        &descriptor_heap_mappings,
                    )?;
                let fragment_stage_builder = vk::PipelineShaderStageCreateInfo::builder()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(fragment_module)
                    .name(shader_entry);
                let fragment_stage = if descriptor_heap_mappings.is_empty() {
                    fragment_stage_builder.build()
                } else {
                    let mut fragment_stage = fragment_stage_builder.build();
                    fragment_stage.next =
                        &mut descriptor_heap_mapping_info as *mut _ as *const std::ffi::c_void;
                    fragment_stage
                };
                let stages = [
                    vk::PipelineShaderStageCreateInfo::builder()
                        .stage(vk::ShaderStageFlags::VERTEX)
                        .module(vertex_module)
                        .name(shader_entry)
                        .build(),
                    fragment_stage,
                ];

                let vertex_bindings = scene_pipeline_vertex_input_bindings(key.vertex_layout);
                let vertex_attributes = scene_pipeline_vertex_input_attributes(key.vertex_layout);
                let vertex_input = vk::PipelineVertexInputStateCreateInfo::builder()
                    .vertex_binding_descriptions(&vertex_bindings)
                    .vertex_attribute_descriptions(&vertex_attributes)
                    .build();
                let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
                    .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
                    .build();
                let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
                    .viewport_count(1)
                    .scissor_count(1)
                    .build();
                let rasterization = vk::PipelineRasterizationStateCreateInfo::builder()
                    .polygon_mode(vk::PolygonMode::FILL)
                    .cull_mode(scene_mesh_cull_mode(key.render_state.cull_mode))
                    .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
                    .line_width(1.0)
                    .build();
                let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
                    .rasterization_samples(vk::SampleCountFlags::_1)
                    .alpha_to_coverage_enable(key.blend == SceneBlendContract::AlphaToCoverage)
                    .build();
                let depth_stencil = scene_mesh_depth_stencil_state(key);
                let color_attachment = scene_mesh_color_blend_attachment(
                    key.blend,
                    key.render_state.alpha_write.writes_alpha(),
                );
                let color_attachments = [color_attachment];
                let color_blend = vk::PipelineColorBlendStateCreateInfo::builder()
                    .attachments(&color_attachments)
                    .build();
                let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
                let dynamic_state = vk::PipelineDynamicStateCreateInfo::builder()
                    .dynamic_states(&dynamic_states)
                    .build();
                let color_attachment_formats = [key.target_format];
                let mut rendering_info = vk::PipelineRenderingCreateInfo::builder()
                    .color_attachment_formats(&color_attachment_formats)
                    .build();
                let mut pipeline_flags2 = vk::PipelineCreateFlags2CreateInfo::builder()
                    .flags(vk::PipelineCreateFlags2::DESCRIPTOR_HEAP_EXT)
                    .build();

                let pipeline_info = vk::GraphicsPipelineCreateInfo::builder()
                    .stages(&stages)
                    .vertex_input_state(&vertex_input)
                    .input_assembly_state(&input_assembly)
                    .viewport_state(&viewport_state)
                    .rasterization_state(&rasterization)
                    .multisample_state(&multisample)
                    .depth_stencil_state(&depth_stencil)
                    .color_blend_state(&color_blend)
                    .dynamic_state(&dynamic_state)
                    .layout(vk::PipelineLayout::null())
                    .render_pass(vk::RenderPass::null())
                    .subpass(0)
                    .push_next(&mut rendering_info)
                    .push_next(&mut pipeline_flags2);
                let pipeline_info = pipeline_info.build();
                let (pipelines, _success_code) = unsafe {
                    device.create_graphics_pipelines(
                        vk::PipelineCache::null(),
                        &[pipeline_info],
                        None,
                    )
                }
                .map_err(|err| format!("vkCreateGraphicsPipelines(scene mesh): {err:?}"))?;
                Ok(NativeVulkanScenePipelineResources {
                    pipeline: pipelines[0],
                    pipeline_layout: vk::PipelineLayout::null(),
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

    result
}

fn scene_pipeline_vertex_stride_bytes(layout: NativeVulkanScenePipelineVertexLayout) -> u32 {
    match layout {
        NativeVulkanScenePipelineVertexLayout::SceneMeshV0 => {
            u32::try_from(SCENE_GPU_MESH_VERTEX_BYTES).unwrap_or(u32::MAX)
        }
        NativeVulkanScenePipelineVertexLayout::FlatTexturePositionUv => 20,
    }
}

fn scene_pipeline_vertex_attribute_labels(
    layout: NativeVulkanScenePipelineVertexLayout,
) -> Vec<&'static str> {
    match layout {
        NativeVulkanScenePipelineVertexLayout::SceneMeshV0 => {
            vec!["location0.xy", "location1.uv", "location2.opacity"]
        }
        NativeVulkanScenePipelineVertexLayout::FlatTexturePositionUv => {
            vec!["location0.xyz", "location1.uv"]
        }
    }
}

fn scene_pipeline_vertex_input_bindings(
    layout: NativeVulkanScenePipelineVertexLayout,
) -> [vk::VertexInputBindingDescription; 1] {
    [vk::VertexInputBindingDescription::builder()
        .binding(0)
        .stride(scene_pipeline_vertex_stride_bytes(layout))
        .input_rate(vk::VertexInputRate::VERTEX)
        .build()]
}

fn scene_pipeline_vertex_input_attributes(
    layout: NativeVulkanScenePipelineVertexLayout,
) -> Vec<vk::VertexInputAttributeDescription> {
    match layout {
        NativeVulkanScenePipelineVertexLayout::SceneMeshV0 => vec![
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
        ],
        NativeVulkanScenePipelineVertexLayout::FlatTexturePositionUv => vec![
            vk::VertexInputAttributeDescription::builder()
                .location(0)
                .binding(0)
                .format(vk::Format::R32G32B32_SFLOAT)
                .offset(0)
                .build(),
            vk::VertexInputAttributeDescription::builder()
                .location(1)
                .binding(0)
                .format(vk::Format::R32G32_SFLOAT)
                .offset(12)
                .build(),
        ],
    }
}

fn scene_mesh_depth_stencil_state(
    key: &NativeVulkanScenePipelineCacheKey,
) -> vk::PipelineDepthStencilStateCreateInfo {
    vk::PipelineDepthStencilStateCreateInfo::builder()
        .depth_test_enable(key.render_state.depth_test.enabled())
        .depth_write_enable(key.render_state.depth_write)
        .depth_compare_op(scene_depth_compare_op(key.render_state.depth_test))
        .build()
}

fn scene_mesh_color_blend_attachment(
    blend: SceneBlendContract,
    write_alpha: bool,
) -> vk::PipelineColorBlendAttachmentState {
    let base = vk::PipelineColorBlendAttachmentState::builder()
        .color_write_mask(scene_color_write_mask(write_alpha));
    match blend {
        SceneBlendContract::NormalReplace
        | SceneBlendContract::AlphaToCoverage
        | SceneBlendContract::ShaderColorBlend(_) => base.blend_enable(false).build(),
        SceneBlendContract::DestColorCopyBackBit0x100 => base
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::DST_COLOR)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE)
            .alpha_blend_op(vk::BlendOp::ADD)
            .build(),
        SceneBlendContract::TranslucentAlpha => base
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD)
            .build(),
        SceneBlendContract::Additive => base
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE)
            .alpha_blend_op(vk::BlendOp::ADD)
            .build(),
    }
}

fn scene_pipeline_blend_label(blend: SceneBlendContract) -> &'static str {
    match blend {
        SceneBlendContract::NormalReplace => "normal-replace-one-zero",
        SceneBlendContract::TranslucentAlpha => "translucent-src-alpha-inv-src-alpha",
        SceneBlendContract::Additive => "additive-src-alpha-one",
        SceneBlendContract::AlphaToCoverage => "alpha-to-coverage-one-zero",
        SceneBlendContract::DestColorCopyBackBit0x100 => {
            "copy-back-dst-color-bit0x100-alpha-one-one"
        }
        SceneBlendContract::ShaderColorBlend(_) => "shader-color-blend-one-zero",
    }
}

fn scene_color_write_mask(write_alpha: bool) -> vk::ColorComponentFlags {
    let rgb = vk::ColorComponentFlags::R | vk::ColorComponentFlags::G | vk::ColorComponentFlags::B;
    if write_alpha {
        rgb | vk::ColorComponentFlags::A
    } else {
        rgb
    }
}

fn scene_alpha_write_label(
    alpha_write: crate::engine::scene_engine::SceneAlphaWriteMode,
) -> &'static str {
    match alpha_write {
        crate::engine::scene_engine::SceneAlphaWriteMode::Default => "default-inherited-rgba",
        crate::engine::scene_engine::SceneAlphaWriteMode::Enabled => "enabled-rgba",
        crate::engine::scene_engine::SceneAlphaWriteMode::Disabled => "disabled-rgb-only",
    }
}

fn scene_depth_test_label(depth_test: SceneDepthTest) -> &'static str {
    match depth_test {
        SceneDepthTest::Disabled => "disabled",
        SceneDepthTest::Less => "less",
        SceneDepthTest::LessEqual => "lessequal",
        SceneDepthTest::Equal => "equal",
        SceneDepthTest::NotEqual => "notequal",
        SceneDepthTest::Greater => "greater",
        SceneDepthTest::Never => "never",
    }
}

fn scene_cull_mode_label(cull_mode: SceneCullMode) -> &'static str {
    match cull_mode {
        SceneCullMode::None => "nocull",
        SceneCullMode::Front => "front",
        SceneCullMode::Back => "back",
    }
}

fn scene_depth_compare_op(depth_test: SceneDepthTest) -> vk::CompareOp {
    match depth_test {
        SceneDepthTest::Disabled | SceneDepthTest::LessEqual => vk::CompareOp::LESS_OR_EQUAL,
        SceneDepthTest::Less => vk::CompareOp::LESS,
        SceneDepthTest::Equal => vk::CompareOp::EQUAL,
        SceneDepthTest::NotEqual => vk::CompareOp::NOT_EQUAL,
        SceneDepthTest::Greater => vk::CompareOp::GREATER,
        SceneDepthTest::Never => vk::CompareOp::NEVER,
    }
}

fn scene_mesh_cull_mode(cull_mode: SceneCullMode) -> vk::CullModeFlags {
    match cull_mode {
        SceneCullMode::None => vk::CullModeFlags::NONE,
        SceneCullMode::Front => vk::CullModeFlags::FRONT,
        SceneCullMode::Back => vk::CullModeFlags::BACK,
    }
}

fn scene_mesh_shader_resource_mapping_labels(
    key: &NativeVulkanScenePipelineCacheKey,
) -> Vec<String> {
    scene_mesh_texture_slots(key.texture_slot_mask)
        .into_iter()
        .enumerate()
        .map(|(ordinal, slot)| {
            if scene_pipeline_uses_alpha_mask_resource_heap(key) {
                format!(
                    "VK_EXT_descriptor_heap set0.binding{slot}.g_Texture{slot} -> alpha-mask-heap-slice-texture-offset{ordinal}"
                )
            } else {
                format!(
                    "VK_EXT_descriptor_heap set0.binding{slot}.g_Texture{slot} -> draw-heap-slice-texture-offset{}",
                    ordinal + 1
                )
            }
        })
        .collect()
}

fn scene_pipeline_descriptor_heap_mappings(
    layout: NativeVulkanSceneMeshPipelineLayoutSpec<'_>,
    key: &NativeVulkanScenePipelineCacheKey,
) -> Result<Vec<NativeVulkanDescriptorHeapShaderBindingMapping>, String> {
    if scene_pipeline_uses_alpha_mask_resource_heap(key) {
        let alpha_mask_plan = layout.layer_alpha_mask_resource_heap_plan.ok_or_else(|| {
            format!(
                "scene alpha-mask pipeline shader '{}' requires alpha-mask descriptor heap mapping",
                key.shader
            )
        })?;
        return scene_alpha_mask_descriptor_heap_mappings(alpha_mask_plan, key);
    }
    scene_mesh_descriptor_heap_mappings(layout.draw_resource_heap_plan, key)
}

fn scene_mesh_descriptor_heap_mappings(
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    key: &NativeVulkanScenePipelineCacheKey,
) -> Result<Vec<NativeVulkanDescriptorHeapShaderBindingMapping>, String> {
    let Some(layout) = scene_mesh_draw_resource_heap_texture_layout(descriptor_heap_plan, key)?
    else {
        return Ok(Vec::new());
    };
    scene_mesh_texture_slots(key.texture_slot_mask)
        .into_iter()
        .enumerate()
        .map(|(ordinal, slot)| {
            native_vulkan_vulkanalia_descriptor_heap_resource_relative_combined_image_sampler_binding_mapping(
                descriptor_heap_plan,
                slot,
                layout.base_resource_descriptor_index,
                layout.base_resource_descriptor_index + 1 + ordinal,
                layout.base_sampler_descriptor_index,
                layout.base_sampler_descriptor_index + ordinal,
            )
        })
        .collect()
}

fn scene_alpha_mask_descriptor_heap_mappings(
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    key: &NativeVulkanScenePipelineCacheKey,
) -> Result<Vec<NativeVulkanDescriptorHeapShaderBindingMapping>, String> {
    let Some(layout) = scene_alpha_mask_resource_heap_texture_layout(descriptor_heap_plan, key)?
    else {
        return Ok(Vec::new());
    };
    scene_mesh_texture_slots(key.texture_slot_mask)
        .into_iter()
        .enumerate()
        .map(|(ordinal, slot)| {
            native_vulkan_vulkanalia_descriptor_heap_resource_relative_sampled_image_binding_mapping(
                descriptor_heap_plan,
                slot,
                layout.base_resource_descriptor_index,
                layout.base_resource_descriptor_index + ordinal,
                layout.base_sampler_descriptor_index,
                layout.base_sampler_descriptor_index + ordinal,
            )
        })
        .collect()
}

fn scene_mesh_texture_slots(texture_slot_mask: u32) -> Vec<u32> {
    (0..u32::BITS)
        .filter(|slot| texture_slot_mask & (1u32 << slot) != 0)
        .collect()
}

fn validate_scene_mesh_pipeline_key(key: &NativeVulkanScenePipelineCacheKey) -> Result<(), String> {
    if key.shader.is_empty() {
        return Err("scene mesh pipeline requires non-empty shader name".to_owned());
    }
    if !key.pipeline_class.is_indexed_mesh_graphics()
        && !key.pipeline_class.is_layer_utility_indexed()
    {
        return Err(format!(
            "scene graphics pipeline requires indexed mesh or layer utility pipeline class, got {:?}",
            key.pipeline_class
        ));
    }
    match (key.pipeline_class, key.vertex_layout) {
        (
            crate::engine::scene_engine::SceneGraphPipelineClass::Mesh
            | crate::engine::scene_engine::SceneGraphPipelineClass::PuppetSkinning,
            NativeVulkanScenePipelineVertexLayout::SceneMeshV0,
        )
        | (
            crate::engine::scene_engine::SceneGraphPipelineClass::LayerUtilityIndexed,
            NativeVulkanScenePipelineVertexLayout::FlatTexturePositionUv,
        ) => {}
        _ => {
            return Err(format!(
                "scene graphics pipeline class {:?} cannot use vertex layout {:?}",
                key.pipeline_class, key.vertex_layout
            ));
        }
    }
    if key.target_format == vk::Format::UNDEFINED {
        return Err("scene mesh pipeline requires defined target format".to_owned());
    }
    let interface = WeShaderInterface::for_shader(&key.shader).ok_or_else(|| {
        format!(
            "scene mesh pipeline references unknown WE shader '{}'",
            key.shader
        )
    })?;
    let _ = interface.texture_slot_mask_for_material(&key.shader, key.texture_slot_mask)?;
    Ok(())
}

fn validate_scene_mesh_descriptor_heap_pipeline_layout(
    layout: NativeVulkanSceneMeshPipelineLayoutSpec<'_>,
    key: &NativeVulkanScenePipelineCacheKey,
) -> Result<(), String> {
    if scene_pipeline_uses_alpha_mask_resource_heap(key) {
        let alpha_mask_plan = layout.layer_alpha_mask_resource_heap_plan.ok_or_else(|| {
            format!(
                "scene alpha-mask pipeline shader '{}' requires alpha-mask descriptor heap mapping",
                key.shader
            )
        })?;
        return scene_alpha_mask_resource_heap_texture_layout(alpha_mask_plan, key).map(|_| ());
    }
    scene_mesh_draw_resource_heap_texture_layout(layout.draw_resource_heap_plan, key).map(|_| ())
}

fn scene_pipeline_uses_alpha_mask_resource_heap(key: &NativeVulkanScenePipelineCacheKey) -> bool {
    key.pipeline_class.is_layer_utility_indexed() || key.shader == "we/clippingmaskimage4"
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SceneMeshDrawResourceHeapTextureLayout {
    base_resource_descriptor_index: usize,
    base_sampler_descriptor_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SceneAlphaMaskResourceHeapTextureLayout {
    base_resource_descriptor_index: usize,
    base_sampler_descriptor_index: usize,
}

fn scene_mesh_draw_resource_heap_texture_layout(
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    key: &NativeVulkanScenePipelineCacheKey,
) -> Result<Option<SceneMeshDrawResourceHeapTextureLayout>, String> {
    let texture_slot_count = key.texture_slot_mask.count_ones() as usize;
    if texture_slot_count == 0 {
        return Ok(None);
    }
    if !descriptor_heap_plan.backend_ready {
        return Err(format!(
            "scene mesh pipeline requires ready draw resource heap mapping: {:?}",
            descriptor_heap_plan.blocking_reason
        ));
    }
    if descriptor_heap_plan.uniform_buffer_count == 0 {
        return Err(
            "scene mesh pipeline requires a WE material uniform descriptor before sampled textures"
                .to_owned(),
        );
    }
    if descriptor_heap_plan.sampled_image_count < texture_slot_count {
        return Err(format!(
            "scene mesh pipeline shader '{}' requires {} WE texture mappings but draw resource heap has {} sampled images",
            key.shader, texture_slot_count, descriptor_heap_plan.sampled_image_count
        ));
    }
    if descriptor_heap_plan.sampler_count < texture_slot_count {
        return Err(format!(
            "scene mesh pipeline shader '{}' requires {} sampler mappings but draw resource heap has {} samplers",
            key.shader, texture_slot_count, descriptor_heap_plan.sampler_count
        ));
    }

    for base_resource_descriptor_index in 0..descriptor_heap_plan.resource_descriptor_kinds.len() {
        if descriptor_heap_plan.resource_descriptor_kinds[base_resource_descriptor_index]
            != NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer
        {
            continue;
        }
        let texture_start = base_resource_descriptor_index + 1;
        let texture_end = texture_start + texture_slot_count;
        if texture_end > descriptor_heap_plan.resource_descriptor_kinds.len() {
            continue;
        }
        if descriptor_heap_plan.resource_descriptor_kinds[texture_start..texture_end]
            .iter()
            .all(|kind| {
                *kind == NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage
            })
        {
            let base_sampler_descriptor_index = descriptor_heap_plan.resource_descriptor_kinds
                [..base_resource_descriptor_index]
                .iter()
                .filter(|kind| {
                    **kind
                        == NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage
                })
                .count();
            return Ok(Some(SceneMeshDrawResourceHeapTextureLayout {
                base_resource_descriptor_index,
                base_sampler_descriptor_index,
            }));
        }
    }

    Err(format!(
        "scene mesh pipeline shader '{}' requires a draw heap slice shaped as WE material uniform + {} sampled textures",
        key.shader, texture_slot_count
    ))
}

fn scene_alpha_mask_resource_heap_texture_layout(
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    key: &NativeVulkanScenePipelineCacheKey,
) -> Result<Option<SceneAlphaMaskResourceHeapTextureLayout>, String> {
    let texture_slot_count = key.texture_slot_mask.count_ones() as usize;
    if texture_slot_count == 0 {
        return Ok(None);
    }
    if !descriptor_heap_plan.backend_ready {
        return Err(format!(
            "scene alpha-mask pipeline requires ready alpha-mask resource heap mapping: {:?}",
            descriptor_heap_plan.blocking_reason
        ));
    }
    if descriptor_heap_plan.sampled_image_count < texture_slot_count {
        return Err(format!(
            "scene layer utility pipeline shader '{}' requires {} alpha-mask texture mappings but heap has {} sampled images",
            key.shader, texture_slot_count, descriptor_heap_plan.sampled_image_count
        ));
    }
    if descriptor_heap_plan.sampler_count < texture_slot_count {
        return Err(format!(
            "scene layer utility pipeline shader '{}' requires {} alpha-mask sampler mappings but heap has {} samplers",
            key.shader, texture_slot_count, descriptor_heap_plan.sampler_count
        ));
    }

    for base_resource_descriptor_index in 0..descriptor_heap_plan.resource_descriptor_kinds.len() {
        let texture_end = base_resource_descriptor_index + texture_slot_count;
        if texture_end > descriptor_heap_plan.resource_descriptor_kinds.len() {
            continue;
        }
        if descriptor_heap_plan.resource_descriptor_kinds
            [base_resource_descriptor_index..texture_end]
            .iter()
            .all(|kind| {
                *kind == NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage
            })
        {
            let base_sampler_descriptor_index = descriptor_heap_plan.resource_descriptor_kinds
                [..base_resource_descriptor_index]
                .iter()
                .filter(|kind| {
                    **kind
                        == NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage
                })
                .count();
            return Ok(Some(SceneAlphaMaskResourceHeapTextureLayout {
                base_resource_descriptor_index,
                base_sampler_descriptor_index,
            }));
        }
    }

    Err(format!(
        "scene alpha-mask pipeline shader '{}' requires an alpha-mask heap slice shaped as {} sampled textures",
        key.shader, texture_slot_count
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::SceneGraphPipelineClass;
    use crate::renderer::native_vulkan::vulkan::{
        NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
        NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
        NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput,
        native_vulkan_vulkanalia_descriptor_heap_resource_plan,
    };

    #[test]
    fn mesh_pipeline_plan_uses_scene_mesh_vertex_layout_and_dynamic_viewport() {
        let plan = native_vulkan_scene_mesh_pipeline_create_plan(&pipeline_key()).unwrap();

        assert_eq!(plan.vertex_stride_bytes, 20);
        assert_eq!(
            plan.vertex_attributes,
            vec!["location0.xy", "location1.uv", "location2.opacity"]
        );
        assert_eq!(plan.dynamic_states, ["viewport", "scissor"]);
        assert_eq!(
            plan.shader_resource_mappings,
            vec![
                "VK_EXT_descriptor_heap set0.binding0.g_Texture0 -> draw-heap-slice-texture-offset1".to_owned()
            ]
        );
        assert_eq!(plan.blend, "translucent-src-alpha-inv-src-alpha");
        assert_eq!(plan.depth_test, "disabled");
        assert_eq!(plan.depth_write, false);
        assert_eq!(plan.cull_mode, "nocull");
        assert_eq!(
            plan.dynamic_rendering_scope,
            "dynamic-rendering-no-render-pass"
        );
    }

    #[test]
    fn mesh_pipeline_plan_maps_additive_blend_contract() {
        let plan =
            native_vulkan_scene_mesh_pipeline_create_plan(&NativeVulkanScenePipelineCacheKey {
                blend: SceneBlendContract::Additive,
                ..pipeline_key()
            })
            .unwrap();

        assert_eq!(plan.blend, "additive-src-alpha-one");
    }

    #[test]
    fn mesh_pipeline_plan_accepts_puppet_skinning_pipeline_class() {
        let plan =
            native_vulkan_scene_mesh_pipeline_create_plan(&NativeVulkanScenePipelineCacheKey {
                pipeline_class: SceneGraphPipelineClass::PuppetSkinning,
                ..pipeline_key()
            })
            .expect("puppet pipeline plan");

        assert_eq!(plan.vertex_stride_bytes, 20);
        assert_eq!(
            plan.vertex_attributes,
            vec!["location0.xy", "location1.uv", "location2.opacity"]
        );
    }

    #[test]
    fn mesh_pipeline_plan_accepts_flattexture_copy_back_utility_pipeline() {
        let plan =
            native_vulkan_scene_mesh_pipeline_create_plan(&NativeVulkanScenePipelineCacheKey {
                shader: "util/minimalalpha".to_owned(),
                blend: SceneBlendContract::DestColorCopyBackBit0x100,
                pipeline_class: SceneGraphPipelineClass::LayerUtilityIndexed,
                vertex_layout: NativeVulkanScenePipelineVertexLayout::FlatTexturePositionUv,
                target_format: vk::Format::R8_UNORM,
                texture_slot_mask: 1,
                ..pipeline_key()
            })
            .expect("flattexture copy-back utility pipeline plan");

        assert_eq!(plan.shader, "util/minimalalpha");
        assert_eq!(plan.target_format, "R8_UNORM");
        assert_eq!(plan.vertex_stride_bytes, 20);
        assert_eq!(
            plan.vertex_attributes,
            vec!["location0.xyz", "location1.uv"]
        );
        assert_eq!(plan.blend, "copy-back-dst-color-bit0x100-alpha-one-one");
        assert_eq!(
            plan.shader_resource_mappings,
            vec![
                "VK_EXT_descriptor_heap set0.binding0.g_Texture0 -> alpha-mask-heap-slice-texture-offset0".to_owned()
            ]
        );
    }

    #[test]
    fn mesh_pipeline_plan_accepts_clippingmaskimage4_alpha_mask_generator() {
        let plan =
            native_vulkan_scene_mesh_pipeline_create_plan(&NativeVulkanScenePipelineCacheKey {
                shader: "we/clippingmaskimage4".to_owned(),
                pipeline_class: SceneGraphPipelineClass::PuppetSkinning,
                target_format: vk::Format::R8_UNORM,
                texture_slot_mask: (1u32 << 0) | (1u32 << 1),
                ..pipeline_key()
            })
            .expect("clipping mask generator pipeline plan");

        assert_eq!(plan.shader, "we/clippingmaskimage4");
        assert_eq!(plan.target_format, "R8_UNORM");
        assert_eq!(plan.blend, "translucent-src-alpha-inv-src-alpha");
        assert_eq!(
            plan.shader_resource_mappings,
            vec![
                "VK_EXT_descriptor_heap set0.binding0.g_Texture0 -> alpha-mask-heap-slice-texture-offset0".to_owned(),
                "VK_EXT_descriptor_heap set0.binding1.g_Texture1 -> alpha-mask-heap-slice-texture-offset1".to_owned()
            ]
        );
    }

    #[test]
    fn mesh_pipeline_plan_rejects_non_indexed_graphics_pipeline_class() {
        let err =
            native_vulkan_scene_mesh_pipeline_create_plan(&NativeVulkanScenePipelineCacheKey {
                pipeline_class: SceneGraphPipelineClass::Quad,
                ..pipeline_key()
            })
            .expect_err("quad pipeline must fail");

        assert!(err.contains("requires indexed mesh or layer utility pipeline class"));
    }

    #[test]
    fn mesh_pipeline_factory_rejects_invalid_spirv_before_device_work() {
        let err = native_vulkan_validate_scene_spirv(&[0, 1, 2, 3], "scene mesh vertex")
            .expect_err("invalid SPIR-V must fail");

        assert!(err.contains("not valid SPIR-V bytecode"));
    }

    #[test]
    fn mesh_pipeline_descriptor_heap_layout_requires_ready_sampled_texture_mapping() {
        let key = NativeVulkanScenePipelineCacheKey {
            texture_slot_mask: 1,
            ..pipeline_key()
        };
        let descriptor_heap_plan = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: vec![
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                ],
                sampler_count: 1,
                properties: NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot::default(),
            },
        );
        let err = validate_scene_mesh_descriptor_heap_pipeline_layout(
            NativeVulkanSceneMeshPipelineLayoutSpec {
                draw_resource_heap_plan: &descriptor_heap_plan,
                layer_alpha_mask_resource_heap_plan: None,
            },
            &key,
        )
        .expect_err("descriptor heap mapping must be ready");

        assert!(err.contains("ready draw resource heap mapping"));
    }

    #[test]
    fn mesh_pipeline_descriptor_heap_layout_accepts_alpha_mask_texture_only_utility_mapping() {
        let key = NativeVulkanScenePipelineCacheKey {
            shader: "util/minimalalpha".to_owned(),
            blend: SceneBlendContract::DestColorCopyBackBit0x100,
            pipeline_class: SceneGraphPipelineClass::LayerUtilityIndexed,
            vertex_layout: NativeVulkanScenePipelineVertexLayout::FlatTexturePositionUv,
            target_format: vk::Format::R8_UNORM,
            texture_slot_mask: 1,
            ..pipeline_key()
        };
        let descriptor_heap_plan = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: vec![
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                ],
                sampler_count: 1,
                properties: descriptor_properties(),
            },
        );

        validate_scene_mesh_descriptor_heap_pipeline_layout(
            NativeVulkanSceneMeshPipelineLayoutSpec {
                draw_resource_heap_plan: &descriptor_heap_plan,
                layer_alpha_mask_resource_heap_plan: Some(&descriptor_heap_plan),
            },
            &key,
        )
        .expect("alpha-mask texture-only utility mapping");
    }

    #[test]
    fn mesh_pipeline_descriptor_heap_layout_accepts_clippingmaskimage4_texture_only_mapping() {
        let key = NativeVulkanScenePipelineCacheKey {
            shader: "we/clippingmaskimage4".to_owned(),
            pipeline_class: SceneGraphPipelineClass::PuppetSkinning,
            vertex_layout: NativeVulkanScenePipelineVertexLayout::SceneMeshV0,
            target_format: vk::Format::R8_UNORM,
            texture_slot_mask: 0b11,
            ..pipeline_key()
        };
        let draw_heap_plan = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: vec![
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                ],
                sampler_count: 1,
                properties: descriptor_properties(),
            },
        );
        let alpha_mask_heap_plan = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: vec![
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                ],
                sampler_count: 2,
                properties: descriptor_properties(),
            },
        );

        validate_scene_mesh_descriptor_heap_pipeline_layout(
            NativeVulkanSceneMeshPipelineLayoutSpec {
                draw_resource_heap_plan: &draw_heap_plan,
                layer_alpha_mask_resource_heap_plan: Some(&alpha_mask_heap_plan),
            },
            &key,
        )
        .expect("clippingmaskimage4 must map through alpha-mask texture-only heap");
    }

    #[test]
    fn mesh_pipeline_descriptor_heap_layout_rejects_clippingmaskimage4_without_alpha_mask_heap() {
        let key = NativeVulkanScenePipelineCacheKey {
            shader: "we/clippingmaskimage4".to_owned(),
            pipeline_class: SceneGraphPipelineClass::PuppetSkinning,
            vertex_layout: NativeVulkanScenePipelineVertexLayout::SceneMeshV0,
            target_format: vk::Format::R8_UNORM,
            texture_slot_mask: 0b11,
            ..pipeline_key()
        };
        let draw_heap_plan = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: vec![
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                ],
                sampler_count: 2,
                properties: descriptor_properties(),
            },
        );

        let err = validate_scene_mesh_descriptor_heap_pipeline_layout(
            NativeVulkanSceneMeshPipelineLayoutSpec {
                draw_resource_heap_plan: &draw_heap_plan,
                layer_alpha_mask_resource_heap_plan: None,
            },
            &key,
        )
        .expect_err("clippingmaskimage4 cannot use ordinary draw heap");

        assert!(err.contains("requires alpha-mask descriptor heap mapping"));
    }

    #[test]
    fn mesh_pipeline_plan_maps_we_texture_slots_to_texture_set_offsets() {
        let plan =
            native_vulkan_scene_mesh_pipeline_create_plan(&NativeVulkanScenePipelineCacheKey {
                texture_slot_mask: 0b1_0001,
                ..pipeline_key()
            })
            .unwrap();

        assert_eq!(
            plan.shader_resource_mappings,
            vec![
                "VK_EXT_descriptor_heap set0.binding0.g_Texture0 -> draw-heap-slice-texture-offset1".to_owned(),
                "VK_EXT_descriptor_heap set0.binding4.g_Texture4 -> draw-heap-slice-texture-offset2".to_owned()
            ]
        );
    }

    fn pipeline_key() -> NativeVulkanScenePipelineCacheKey {
        NativeVulkanScenePipelineCacheKey {
            shader: "we/genericimage4".to_owned(),
            blend: SceneBlendContract::TranslucentAlpha,
            render_state: crate::engine::scene_engine::SceneMaterialRenderState::translucent_2d(),
            pipeline_class: SceneGraphPipelineClass::Mesh,
            vertex_layout: NativeVulkanScenePipelineVertexLayout::SceneMeshV0,
            target_format: vk::Format::B8G8R8A8_UNORM,
            texture_slot_mask: 1,
        }
    }

    fn descriptor_properties() -> NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
        NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot {
            resource_heap_alignment: 32,
            sampler_heap_alignment: 16,
            max_resource_heap_size: 1024,
            max_sampler_heap_size: 1024,
            min_sampler_heap_reserved_range: 16,
            min_sampler_heap_reserved_range_with_embedded: 16,
            min_resource_heap_reserved_range: 32,
            sampler_descriptor_size: 16,
            image_descriptor_size: 32,
            buffer_descriptor_size: 32,
            sampler_descriptor_alignment: 16,
            image_descriptor_alignment: 32,
            buffer_descriptor_alignment: 32,
            max_push_data_size: 0,
            max_descriptor_heap_embedded_samplers: 0,
            sampler_ycbcr_conversion_count: 0,
            sparse_descriptor_heaps: false,
            protected_descriptor_heaps: false,
        }
    }
}
