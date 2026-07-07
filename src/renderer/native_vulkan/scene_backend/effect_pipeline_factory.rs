//! Scene effect graphics pipeline factory.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/effect-semantics.md`
//! - `reverse-engineered/effects/iris.md`
//! - `reverse-engineered/effects/fluidsimulation.md`
//! - `references/godot/servers/rendering/renderer_rd/effects/tone_mapper.cpp`
//! - `references/godot/servers/rendering/renderer_rd/effects/copy_effects.cpp`
//! - `references/godot/servers/rendering/renderer_rd/pipeline_hash_map_rd.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SceneAlphaWriteMode, SceneCullMode, SceneDepthTest, SceneEffectPassBlend,
};
use crate::renderer::native_vulkan::vulkan::{
    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_sampled_image_binding_mapping,
};

use super::effect_pipeline::{
    NativeVulkanSceneEffectPipelineKey, NativeVulkanSceneEffectPipelineShaders,
};
use super::pipeline::NativeVulkanScenePipelineResources;
use super::shader_module::native_vulkan_create_scene_shader_module;

#[derive(Debug, Clone, Copy)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectPipelineLayoutSpec<'a> {
    pub effect_resource_heap_plan: &'a NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneEffectPipelineCreatePlan {
    pub shader: String,
    pub target_format: String,
    pub raster_geometry: &'static str,
    pub vertex_input: &'static str,
    pub dynamic_states: [&'static str; 2],
    pub shader_resource_mappings: Vec<String>,
    pub blend: &'static str,
    pub depth_test: &'static str,
    pub depth_write: bool,
    pub cull_mode: &'static str,
    pub alpha_write: &'static str,
    pub dynamic_rendering_scope: &'static str,
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_scene_effect_pipeline_create_plan(
    key: &NativeVulkanSceneEffectPipelineKey<'_>,
) -> Result<NativeVulkanSceneEffectPipelineCreatePlan, String> {
    validate_scene_effect_pipeline_key(key)?;
    Ok(NativeVulkanSceneEffectPipelineCreatePlan {
        shader: key.shader.to_owned(),
        target_format: format!("{:?}", key.target_format),
        raster_geometry: "fullscreen-triangle",
        vertex_input: "none",
        dynamic_states: ["viewport", "scissor"],
        shader_resource_mappings: scene_effect_shader_resource_mapping_labels(key),
        blend: scene_effect_blend_label(key.blend),
        depth_test: scene_depth_test_label(key.depth_test),
        depth_write: key.depth_write,
        cull_mode: scene_cull_mode_label(key.cull_mode),
        alpha_write: scene_alpha_write_label(key.alpha_write),
        dynamic_rendering_scope: "dynamic-rendering-no-render-pass",
    })
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_create_scene_effect_pipeline_resources(
    device: &Device,
    key: &NativeVulkanSceneEffectPipelineKey<'_>,
    shaders: NativeVulkanSceneEffectPipelineShaders<'_>,
    layout: NativeVulkanSceneEffectPipelineLayoutSpec<'_>,
) -> Result<NativeVulkanScenePipelineResources, String> {
    validate_scene_effect_pipeline_key(key)?;
    validate_scene_effect_descriptor_heap_pipeline_layout(layout.effect_resource_heap_plan, key)?;

    let result = (|| -> Result<NativeVulkanScenePipelineResources, String> {
        let vertex_module = native_vulkan_create_scene_shader_module(
            device,
            shaders.vertex_spirv,
            "scene effect vertex",
        )?;
        let result = (|| -> Result<NativeVulkanScenePipelineResources, String> {
            let fragment_module = native_vulkan_create_scene_shader_module(
                device,
                shaders.fragment_spirv,
                "scene effect fragment",
            )?;
            let result = (|| -> Result<NativeVulkanScenePipelineResources, String> {
                let shader_entry = b"main\0";
                let descriptor_heap_mappings =
                    scene_effect_descriptor_heap_mappings(layout.effect_resource_heap_plan, key)?;
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

                let vertex_input = vk::PipelineVertexInputStateCreateInfo::builder().build();
                let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::builder()
                    .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
                    .build();
                let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
                    .viewport_count(1)
                    .scissor_count(1)
                    .build();
                let rasterization = vk::PipelineRasterizationStateCreateInfo::builder()
                    .polygon_mode(vk::PolygonMode::FILL)
                    .cull_mode(scene_effect_cull_mode(key.cull_mode))
                    .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
                    .line_width(1.0)
                    .build();
                let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
                    .rasterization_samples(vk::SampleCountFlags::_1)
                    .alpha_to_coverage_enable(key.blend == SceneEffectPassBlend::AlphaToCoverage)
                    .build();
                let depth_stencil = scene_effect_depth_stencil_state(key);
                let color_attachment =
                    scene_effect_color_blend_attachment(key.blend, key.alpha_write.writes_alpha());
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
                .map_err(|err| format!("vkCreateGraphicsPipelines(scene effect): {err:?}"))?;
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

fn validate_scene_effect_pipeline_key(
    key: &NativeVulkanSceneEffectPipelineKey<'_>,
) -> Result<(), String> {
    if key.shader.is_empty() {
        return Err("scene effect pipeline requires non-empty shader name".to_owned());
    }
    if !key.shader.starts_with("effects/") && !key.shader.starts_with("util/") {
        return Err(format!(
            "scene effect pipeline shader '{}' must use the effects/ or util/ namespace",
            key.shader
        ));
    }
    if key.target_format == vk::Format::UNDEFINED {
        return Err("scene effect pipeline requires defined target format".to_owned());
    }
    if key.blend == SceneEffectPassBlend::Unknown {
        return Err(format!(
            "scene effect pipeline shader '{}' has unknown WE blend state",
            key.shader
        ));
    }
    Ok(())
}

fn validate_scene_effect_descriptor_heap_pipeline_layout(
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    key: &NativeVulkanSceneEffectPipelineKey<'_>,
) -> Result<(), String> {
    scene_effect_resource_heap_texture_layout(descriptor_heap_plan, key).map(|_| ())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SceneEffectResourceHeapTextureLayout {
    base_resource_descriptor_index: usize,
    base_sampler_descriptor_index: usize,
}

fn scene_effect_resource_heap_texture_layout(
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    key: &NativeVulkanSceneEffectPipelineKey<'_>,
) -> Result<Option<SceneEffectResourceHeapTextureLayout>, String> {
    let texture_slot_count = key.texture_slot_mask.count_ones() as usize;
    if texture_slot_count == 0 {
        return Ok(None);
    }
    if !descriptor_heap_plan.backend_ready {
        return Err(format!(
            "scene effect pipeline requires ready effect resource heap mapping: {:?}",
            descriptor_heap_plan.blocking_reason
        ));
    }
    if descriptor_heap_plan.sampled_image_count < texture_slot_count {
        return Err(format!(
            "scene effect pipeline shader '{}' requires {} WE texture mappings but effect resource heap has {} sampled images",
            key.shader, texture_slot_count, descriptor_heap_plan.sampled_image_count
        ));
    }
    if descriptor_heap_plan.sampler_count < texture_slot_count {
        return Err(format!(
            "scene effect pipeline shader '{}' requires {} sampler mappings but effect resource heap has {} samplers",
            key.shader, texture_slot_count, descriptor_heap_plan.sampler_count
        ));
    }
    if descriptor_heap_plan
        .resource_descriptor_kinds
        .iter()
        .take(texture_slot_count)
        .all(|kind| {
            *kind == NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage
        })
    {
        return Ok(Some(SceneEffectResourceHeapTextureLayout {
            base_resource_descriptor_index: 0,
            base_sampler_descriptor_index: 0,
        }));
    }
    Err(format!(
        "scene effect pipeline shader '{}' requires a resource-set slice shaped as {} sampled textures",
        key.shader, texture_slot_count
    ))
}

fn scene_effect_descriptor_heap_mappings(
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    key: &NativeVulkanSceneEffectPipelineKey<'_>,
) -> Result<Vec<vk::DescriptorSetAndBindingMappingEXT>, String> {
    let Some(layout) = scene_effect_resource_heap_texture_layout(descriptor_heap_plan, key)? else {
        return Ok(Vec::new());
    };
    scene_effect_texture_slots(key.texture_slot_mask)
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

fn scene_effect_shader_resource_mapping_labels(
    key: &NativeVulkanSceneEffectPipelineKey<'_>,
) -> Vec<String> {
    scene_effect_texture_slots(key.texture_slot_mask)
        .into_iter()
        .enumerate()
        .map(|(ordinal, slot)| {
            format!(
                "VK_EXT_descriptor_heap set0.binding{slot}.g_Texture{slot} -> effect-resource-set-texture-offset{ordinal}"
            )
        })
        .collect()
}

fn scene_effect_texture_slots(texture_slot_mask: u32) -> Vec<u32> {
    (0..u32::BITS)
        .filter(|slot| texture_slot_mask & (1u32 << slot) != 0)
        .collect()
}

fn scene_effect_depth_stencil_state(
    key: &NativeVulkanSceneEffectPipelineKey<'_>,
) -> vk::PipelineDepthStencilStateCreateInfo {
    vk::PipelineDepthStencilStateCreateInfo::builder()
        .depth_test_enable(key.depth_test.enabled())
        .depth_write_enable(key.depth_write)
        .depth_compare_op(scene_depth_compare_op(key.depth_test))
        .build()
}

fn scene_effect_color_blend_attachment(
    blend: SceneEffectPassBlend,
    write_alpha: bool,
) -> vk::PipelineColorBlendAttachmentState {
    let base = vk::PipelineColorBlendAttachmentState::builder()
        .color_write_mask(scene_color_write_mask(write_alpha));
    match blend {
        SceneEffectPassBlend::NormalReplace
        | SceneEffectPassBlend::AlphaToCoverage
        | SceneEffectPassBlend::Disabled
        | SceneEffectPassBlend::Unknown => base.blend_enable(false).build(),
        SceneEffectPassBlend::TranslucentAlpha => base
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD)
            .build(),
        SceneEffectPassBlend::Additive => base
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

fn scene_effect_blend_label(blend: SceneEffectPassBlend) -> &'static str {
    match blend {
        SceneEffectPassBlend::NormalReplace => "normal-replace-one-zero",
        SceneEffectPassBlend::TranslucentAlpha => "translucent-src-alpha-inv-src-alpha",
        SceneEffectPassBlend::Additive => "additive-src-alpha-one",
        SceneEffectPassBlend::AlphaToCoverage => "alpha-to-coverage-one-zero",
        SceneEffectPassBlend::Disabled => "disabled-one-zero",
        SceneEffectPassBlend::Unknown => "unknown",
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

fn scene_alpha_write_label(alpha_write: SceneAlphaWriteMode) -> &'static str {
    match alpha_write {
        SceneAlphaWriteMode::Default => "default-rgb-only",
        SceneAlphaWriteMode::Enabled => "enabled-rgba",
        SceneAlphaWriteMode::Disabled => "disabled-rgb-only",
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

fn scene_effect_cull_mode(cull_mode: SceneCullMode) -> vk::CullModeFlags {
    match cull_mode {
        SceneCullMode::None => vk::CullModeFlags::NONE,
        SceneCullMode::Front => vk::CullModeFlags::FRONT,
        SceneCullMode::Back => vk::CullModeFlags::BACK,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::we::WeEffectKind;
    use crate::renderer::native_vulkan::vulkan::{
        NativeVulkanVulkanaliaDescriptorHeapPropertySnapshot,
        NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput,
        native_vulkan_vulkanalia_descriptor_heap_resource_plan,
    };

    #[test]
    fn effect_pipeline_plan_uses_fullscreen_triangle_and_effect_heap_mapping() {
        let plan = native_vulkan_scene_effect_pipeline_create_plan(&effect_key()).unwrap();

        assert_eq!(plan.shader, "effects/iris");
        assert_eq!(plan.raster_geometry, "fullscreen-triangle");
        assert_eq!(plan.vertex_input, "none");
        assert_eq!(plan.dynamic_states, ["viewport", "scissor"]);
        assert_eq!(
            plan.shader_resource_mappings,
            vec![
                "VK_EXT_descriptor_heap set0.binding0.g_Texture0 -> effect-resource-set-texture-offset0".to_owned(),
                "VK_EXT_descriptor_heap set0.binding2.g_Texture2 -> effect-resource-set-texture-offset1".to_owned(),
            ]
        );
        assert_eq!(plan.blend, "normal-replace-one-zero");
        assert_eq!(plan.depth_test, "disabled");
        assert_eq!(plan.depth_write, false);
        assert_eq!(plan.cull_mode, "nocull");
    }

    #[test]
    fn effect_pipeline_plan_accepts_util_passthrough_materials() {
        let mut key = effect_key();
        key.shader = "util/passthrough";

        let plan = native_vulkan_scene_effect_pipeline_create_plan(&key)
            .expect("util passthrough is a valid WE utility shader");

        assert_eq!(plan.shader, "util/passthrough");
        assert_eq!(plan.raster_geometry, "fullscreen-triangle");
    }

    #[test]
    fn effect_pipeline_descriptor_heap_layout_accepts_sampled_image_set() {
        let descriptor_heap_plan = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: vec![
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                ],
                sampler_count: 2,
                properties: descriptor_properties(),
            },
        );

        let layout =
            scene_effect_resource_heap_texture_layout(&descriptor_heap_plan, &effect_key())
                .expect("effect descriptor heap layout")
                .expect("texture layout");

        assert_eq!(layout.base_resource_descriptor_index, 0);
        assert_eq!(layout.base_sampler_descriptor_index, 0);
    }

    #[test]
    fn effect_pipeline_descriptor_heap_layout_rejects_mesh_uniform_base() {
        let descriptor_heap_plan = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
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

        let err = scene_effect_resource_heap_texture_layout(&descriptor_heap_plan, &effect_key())
            .expect_err("mesh-shaped resource set must fail");

        assert!(err.contains("sampled textures"));
    }

    fn effect_key() -> NativeVulkanSceneEffectPipelineKey<'static> {
        NativeVulkanSceneEffectPipelineKey {
            shader: "effects/iris",
            effect: WeEffectKind::Iris,
            blend: SceneEffectPassBlend::NormalReplace,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            alpha_write: SceneAlphaWriteMode::Default,
            target_format: vk::Format::R16G16B16A16_SFLOAT,
            texture_slot_mask: 0b101,
            raster_geometry: super::super::effect_pipeline::NativeVulkanSceneEffectRasterGeometry::FullscreenTriangle,
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
