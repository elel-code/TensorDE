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

use crate::engine::scene_engine::{SCENE_GPU_MESH_VERTEX_BYTES, SceneBlendContract};
use crate::renderer::native_vulkan::vulkan::{
    NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_binding_mapping,
};

use super::pipeline::{
    NativeVulkanScenePipelineCacheKey, NativeVulkanScenePipelineResources,
    NativeVulkanScenePipelineVertexLayout,
};

#[derive(Debug, Clone, Copy)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMeshPipelineShaders<'a> {
    pub vertex_spirv: &'a [u32],
    pub fragment_spirv: &'a [u32],
}

#[derive(Debug, Clone, Copy)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMeshPipelineLayoutSpec<'a> {
    pub texture_descriptor_heap_plan:
        &'a NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneMeshPipelineCreatePlan {
    pub shader: String,
    pub target_format: String,
    pub vertex_stride_bytes: u32,
    pub vertex_attributes: [&'static str; 3],
    pub dynamic_states: [&'static str; 2],
    pub shader_resource_mappings: Vec<String>,
    pub blend: &'static str,
    pub depth_test: bool,
    pub depth_write: bool,
    pub dynamic_rendering_scope: &'static str,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_mesh_pipeline_create_plan(
    key: &NativeVulkanScenePipelineCacheKey,
) -> Result<NativeVulkanSceneMeshPipelineCreatePlan, String> {
    validate_scene_mesh_pipeline_key(key)?;
    Ok(NativeVulkanSceneMeshPipelineCreatePlan {
        shader: key.shader.clone(),
        target_format: format!("{:?}", key.target_format),
        vertex_stride_bytes: u32::try_from(SCENE_GPU_MESH_VERTEX_BYTES).unwrap_or(u32::MAX),
        vertex_attributes: ["location0.xy", "location1.uv", "location2.opacity"],
        dynamic_states: ["viewport", "scissor"],
        shader_resource_mappings: scene_mesh_shader_resource_mapping_labels(key),
        blend: scene_pipeline_blend_label(key.blend),
        depth_test: key.tests_depth,
        depth_write: key.writes_depth,
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
    validate_spirv(shaders.vertex_spirv, "scene mesh vertex")?;
    validate_spirv(shaders.fragment_spirv, "scene mesh fragment")?;
    validate_scene_mesh_descriptor_heap_pipeline_layout(layout.texture_descriptor_heap_plan, key)?;

    let result = (|| -> Result<NativeVulkanScenePipelineResources, String> {
        let vertex_module =
            create_scene_shader_module(device, shaders.vertex_spirv, "scene mesh vertex")?;
        let result = (|| -> Result<NativeVulkanScenePipelineResources, String> {
            let fragment_module =
                create_scene_shader_module(device, shaders.fragment_spirv, "scene mesh fragment")?;
            let result = (|| -> Result<NativeVulkanScenePipelineResources, String> {
                let shader_entry = b"main\0";
                let descriptor_heap_mappings =
                    scene_mesh_descriptor_heap_mappings(layout.texture_descriptor_heap_plan, key)?;
                let mut descriptor_heap_mapping_info =
                    vk::ShaderDescriptorSetAndBindingMappingInfoEXT::builder()
                        .mappings(&descriptor_heap_mappings)
                        .build();
                let fragment_stage_builder = vk::PipelineShaderStageCreateInfo::builder()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(fragment_module)
                    .name(shader_entry);
                let fragment_stage = if descriptor_heap_mappings.is_empty() {
                    fragment_stage_builder.build()
                } else {
                    fragment_stage_builder
                        .push_next(&mut descriptor_heap_mapping_info)
                        .build()
                };
                let stages = [
                    vk::PipelineShaderStageCreateInfo::builder()
                        .stage(vk::ShaderStageFlags::VERTEX)
                        .module(vertex_module)
                        .name(shader_entry)
                        .build(),
                    fragment_stage,
                ];

                let vertex_bindings = scene_mesh_vertex_input_bindings();
                let vertex_attributes = scene_mesh_vertex_input_attributes();
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
                    .cull_mode(vk::CullModeFlags::NONE)
                    .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
                    .line_width(1.0)
                    .build();
                let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
                    .rasterization_samples(vk::SampleCountFlags::_1)
                    .build();
                let depth_stencil = scene_mesh_depth_stencil_state(key);
                let color_attachment = scene_mesh_color_blend_attachment(key.blend);
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

fn scene_mesh_vertex_input_bindings() -> [vk::VertexInputBindingDescription; 1] {
    [vk::VertexInputBindingDescription::builder()
        .binding(0)
        .stride(u32::try_from(SCENE_GPU_MESH_VERTEX_BYTES).unwrap_or(u32::MAX))
        .input_rate(vk::VertexInputRate::VERTEX)
        .build()]
}

fn scene_mesh_vertex_input_attributes() -> [vk::VertexInputAttributeDescription; 3] {
    [
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
    ]
}

fn scene_mesh_depth_stencil_state(
    key: &NativeVulkanScenePipelineCacheKey,
) -> vk::PipelineDepthStencilStateCreateInfo {
    vk::PipelineDepthStencilStateCreateInfo::builder()
        .depth_test_enable(key.tests_depth)
        .depth_write_enable(key.writes_depth)
        .depth_compare_op(vk::CompareOp::LESS_OR_EQUAL)
        .build()
}

fn scene_mesh_color_blend_attachment(
    blend: SceneBlendContract,
) -> vk::PipelineColorBlendAttachmentState {
    let base = vk::PipelineColorBlendAttachmentState::builder().color_write_mask(
        vk::ColorComponentFlags::R
            | vk::ColorComponentFlags::G
            | vk::ColorComponentFlags::B
            | vk::ColorComponentFlags::A,
    );
    match blend {
        SceneBlendContract::NormalReplace | SceneBlendContract::ShaderColorBlend(_) => {
            base.blend_enable(false).build()
        }
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
        SceneBlendContract::ShaderColorBlend(_) => "shader-color-blend-one-zero",
    }
}

fn scene_mesh_shader_resource_mapping_labels(
    key: &NativeVulkanScenePipelineCacheKey,
) -> Vec<String> {
    scene_mesh_texture_slots(key.texture_slot_mask)
        .into_iter()
        .enumerate()
        .map(|(ordinal, slot)| {
            format!(
                "VK_EXT_descriptor_heap set0.binding{slot}.g_Texture{slot} -> texture-set-offset{ordinal}"
            )
        })
        .collect()
}

fn scene_mesh_descriptor_heap_mappings(
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    key: &NativeVulkanScenePipelineCacheKey,
) -> Result<Vec<vk::DescriptorSetAndBindingMappingEXT>, String> {
    validate_scene_mesh_descriptor_heap_pipeline_layout(descriptor_heap_plan, key)?;
    scene_mesh_texture_slots(key.texture_slot_mask)
        .into_iter()
        .enumerate()
        .map(|(ordinal, slot)| {
            native_vulkan_vulkanalia_descriptor_heap_combined_image_sampler_binding_mapping(
                descriptor_heap_plan,
                slot,
                ordinal,
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
    if key.pipeline_class != crate::engine::scene_engine::SceneGraphPipelineClass::Mesh {
        return Err(format!(
            "scene mesh pipeline requires Mesh pipeline class, got {:?}",
            key.pipeline_class
        ));
    }
    if key.vertex_layout != NativeVulkanScenePipelineVertexLayout::SceneMeshV0 {
        return Err(format!(
            "scene mesh pipeline requires SceneMeshV0 vertex layout, got {:?}",
            key.vertex_layout
        ));
    }
    if key.target_format == vk::Format::UNDEFINED {
        return Err("scene mesh pipeline requires defined target format".to_owned());
    }
    Ok(())
}

fn validate_scene_mesh_descriptor_heap_pipeline_layout(
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot,
    key: &NativeVulkanScenePipelineCacheKey,
) -> Result<(), String> {
    let texture_slot_count = key.texture_slot_mask.count_ones() as usize;
    if texture_slot_count == 0 {
        return Ok(());
    }
    if !descriptor_heap_plan.backend_ready {
        return Err(format!(
            "scene mesh pipeline requires ready texture descriptor heap mapping: {:?}",
            descriptor_heap_plan.blocking_reason
        ));
    }
    if descriptor_heap_plan.image_count == 0 {
        return Err(
            "scene mesh pipeline requires at least one sampled texture descriptor".to_owned(),
        );
    }
    if descriptor_heap_plan.image_count < texture_slot_count {
        return Err(format!(
            "scene mesh pipeline shader '{}' requires {} WE texture mappings but descriptor heap has {} images",
            key.shader, texture_slot_count, descriptor_heap_plan.image_count
        ));
    }
    Ok(())
}

fn create_scene_shader_module(
    device: &Device,
    code: &[u32],
    label: &'static str,
) -> Result<vk::ShaderModule, String> {
    validate_spirv(code, label)?;
    let create_info = vk::ShaderModuleCreateInfo::builder()
        .code(code)
        .code_size(std::mem::size_of_val(code));
    unsafe { device.create_shader_module(&create_info, None) }
        .map_err(|err| format!("vkCreateShaderModule({label}): {err:?}"))
}

fn validate_spirv(code: &[u32], label: &'static str) -> Result<(), String> {
    if code.first().copied() != Some(0x0723_0203) {
        return Err(format!("{label} shader is not valid SPIR-V bytecode"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::SceneGraphPipelineClass;

    #[test]
    fn mesh_pipeline_plan_uses_scene_mesh_vertex_layout_and_dynamic_viewport() {
        let plan = native_vulkan_scene_mesh_pipeline_create_plan(&pipeline_key()).unwrap();

        assert_eq!(plan.vertex_stride_bytes, 20);
        assert_eq!(
            plan.vertex_attributes,
            ["location0.xy", "location1.uv", "location2.opacity"]
        );
        assert_eq!(plan.dynamic_states, ["viewport", "scissor"]);
        assert_eq!(plan.shader_resource_mappings, Vec::<String>::new());
        assert_eq!(plan.blend, "translucent-src-alpha-inv-src-alpha");
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
    fn mesh_pipeline_plan_rejects_non_mesh_pipeline_class() {
        let err =
            native_vulkan_scene_mesh_pipeline_create_plan(&NativeVulkanScenePipelineCacheKey {
                pipeline_class: SceneGraphPipelineClass::Quad,
                ..pipeline_key()
            })
            .expect_err("non-mesh pipeline must fail");

        assert!(err.contains("requires Mesh pipeline class"));
    }

    #[test]
    fn mesh_pipeline_factory_rejects_invalid_spirv_before_device_work() {
        let err = validate_spirv(&[0, 1, 2, 3], "scene mesh vertex")
            .expect_err("invalid SPIR-V must fail");

        assert!(err.contains("not valid SPIR-V bytecode"));
    }

    #[test]
    fn mesh_pipeline_descriptor_heap_layout_requires_ready_sampled_texture_mapping() {
        let key = NativeVulkanScenePipelineCacheKey {
            texture_slot_mask: 1,
            ..pipeline_key()
        };
        let err = validate_scene_mesh_descriptor_heap_pipeline_layout(
            &NativeVulkanVulkanaliaDescriptorHeapImageSamplerPlanSnapshot {
                binding: "vulkanalia",
                route: "descriptor-heap-image-sampler-plan",
                descriptor_model: "VK_EXT_descriptor_heap",
                backend_ready: false,
                blocking_reason: Some("no-sampled-images"),
                image_count: 0,
                resource_heap_alignment: 0,
                sampler_heap_alignment: 0,
                image_descriptor_size: 0,
                sampler_descriptor_size: 0,
                image_descriptor_stride: 0,
                sampler_descriptor_stride: 0,
                resource_heap_bytes: 0,
                sampler_heap_bytes: 0,
                resource_heap_reserved_range_offset: 0,
                resource_heap_reserved_range_size: 0,
                sampler_heap_reserved_range_offset: 0,
                sampler_heap_reserved_range_size: 0,
                image_descriptor_offsets: Vec::new(),
                sampler_descriptor_offsets: Vec::new(),
                max_resource_heap_size: 0,
                max_sampler_heap_size: 0,
                command_order: Vec::new(),
                next_gate: "test",
                primary_reference: "test",
            },
            &key,
        )
        .expect_err("descriptor heap mapping must be ready");

        assert!(err.contains("ready texture descriptor heap mapping"));
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
                "VK_EXT_descriptor_heap set0.binding0.g_Texture0 -> texture-set-offset0".to_owned(),
                "VK_EXT_descriptor_heap set0.binding4.g_Texture4 -> texture-set-offset1".to_owned()
            ]
        );
    }

    fn pipeline_key() -> NativeVulkanScenePipelineCacheKey {
        NativeVulkanScenePipelineCacheKey {
            shader: "we/genericimage4".to_owned(),
            blend: SceneBlendContract::TranslucentAlpha,
            writes_depth: false,
            tests_depth: false,
            pipeline_class: SceneGraphPipelineClass::Mesh,
            vertex_layout: NativeVulkanScenePipelineVertexLayout::SceneMeshV0,
            target_format: vk::Format::B8G8R8A8_UNORM,
            texture_slot_mask: 0,
        }
    }
}
