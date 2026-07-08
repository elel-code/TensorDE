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

use std::collections::BTreeSet;

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::{
    SceneAlphaWriteMode, SceneCullMode, SceneDepthTest, SceneEffectPassBlend, WeShaderInterface,
};
use crate::renderer::native_vulkan::vulkan::{
    NativeVulkanDescriptorHeapShaderBindingMapping,
    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind,
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_combined_image_sampler_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_sampled_image_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_uniform_buffer_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping_info,
};

use super::effect_pipeline::{
    NativeVulkanSceneEffectPipelineKey, NativeVulkanSceneEffectPipelineShaders,
};
use super::pipeline::NativeVulkanScenePipelineResources;
use super::shader_module::native_vulkan_create_scene_shader_module;
use super::shader_reflection::{
    NativeVulkanSceneSpirvResourceReflection, native_vulkan_reflect_scene_spirv_resources,
};

#[derive(Debug, Clone, Copy)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectPipelineLayoutSpec<'a> {
    pub effect_resource_heap_plan: &'a NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneEffectPipelineCreatePlan {
    pub shader: String,
    pub shader_combo_values: Vec<String>,
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
        shader_combo_values: scene_effect_shader_combo_labels(key),
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
                let descriptor_heap_mappings = scene_effect_descriptor_heap_stage_mappings(
                    layout.effect_resource_heap_plan,
                    key,
                    shaders,
                )?;
                let mut vertex_descriptor_heap_mapping_info =
                    native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping_info(
                        &descriptor_heap_mappings.vertex,
                    )?;
                let mut fragment_descriptor_heap_mapping_info =
                    native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping_info(
                        &descriptor_heap_mappings.fragment,
                    )?;
                let vertex_stage_builder = vk::PipelineShaderStageCreateInfo::builder()
                    .stage(vk::ShaderStageFlags::VERTEX)
                    .module(vertex_module)
                    .name(shader_entry);
                let vertex_stage = if descriptor_heap_mappings.vertex.is_empty() {
                    vertex_stage_builder.build()
                } else {
                    let mut vertex_stage = vertex_stage_builder.build();
                    vertex_stage.next = &mut vertex_descriptor_heap_mapping_info as *mut _
                        as *const std::ffi::c_void;
                    vertex_stage
                };
                let fragment_stage_builder = vk::PipelineShaderStageCreateInfo::builder()
                    .stage(vk::ShaderStageFlags::FRAGMENT)
                    .module(fragment_module)
                    .name(shader_entry);
                let fragment_stage = if descriptor_heap_mappings.fragment.is_empty() {
                    fragment_stage_builder.build()
                } else {
                    let mut fragment_stage = fragment_stage_builder.build();
                    fragment_stage.next = &mut fragment_descriptor_heap_mapping_info as *mut _
                        as *const std::ffi::c_void;
                    fragment_stage
                };
                let stages = [vertex_stage, fragment_stage];

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
    if let Some(interface) = WeShaderInterface::for_effect_shader(key.shader) {
        interface.texture_slot_mask_for_material(key.shader, key.texture_slot_mask)?;
        for combo in &key.shader_combo_values {
            if !interface.declares_combo(&combo.name) {
                return Err(format!(
                    "scene effect pipeline shader '{}' references undeclared WE combo '{}'",
                    key.shader, combo.name
                ));
            }
        }
        validate_scene_effect_pipeline_iris_key(key)?;
    }
    Ok(())
}

fn validate_scene_effect_pipeline_iris_key(
    key: &NativeVulkanSceneEffectPipelineKey<'_>,
) -> Result<(), String> {
    if key.shader != "effects/iris" {
        return Ok(());
    }
    let mask_combo_enabled = key
        .shader_combo_values
        .iter()
        .any(|combo| combo.name == "MASK" && combo.value != 0);
    let background_combo_enabled = key
        .shader_combo_values
        .iter()
        .any(|combo| combo.name == "BACKGROUND" && combo.value != 0);
    let mask_texture_bound = key.texture_slot_mask & (1u32 << 1) != 0;
    if key.effect_uniform_buffer_count != 2 {
        return Err(format!(
            "scene effect pipeline shader 'effects/iris' requires 2 stage-split effect uniform buffers, got {}",
            key.effect_uniform_buffer_count
        ));
    }
    if mask_combo_enabled && !mask_texture_bound {
        return Err(
            "scene effect pipeline shader 'effects/iris' enables MASK but has no g_Texture1 WE slot 1 binding"
                .to_owned(),
        );
    }
    if mask_texture_bound && !mask_combo_enabled {
        return Err(
            "scene effect pipeline shader 'effects/iris' binds g_Texture1 WE slot 1 but MASK combo is not enabled"
                .to_owned(),
        );
    }
    if background_combo_enabled && !mask_combo_enabled {
        return Err(
            "scene effect pipeline shader 'effects/iris' enables BACKGROUND without MASK"
                .to_owned(),
        );
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
    uniform_buffer_count: usize,
}

#[derive(Debug, Clone)]
struct SceneEffectDescriptorHeapStageMappings {
    vertex: Vec<NativeVulkanDescriptorHeapShaderBindingMapping>,
    fragment: Vec<NativeVulkanDescriptorHeapShaderBindingMapping>,
}

fn scene_effect_resource_heap_texture_layout(
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    key: &NativeVulkanSceneEffectPipelineKey<'_>,
) -> Result<Option<SceneEffectResourceHeapTextureLayout>, String> {
    let texture_slot_count = key.texture_slot_mask.count_ones() as usize;
    let uniform_buffer_count = key.effect_uniform_buffer_count;
    if texture_slot_count == 0 && uniform_buffer_count == 0 {
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
    for base_resource_descriptor_index in 0..descriptor_heap_plan.resource_descriptor_kinds.len() {
        let texture_start = base_resource_descriptor_index.saturating_add(uniform_buffer_count);
        let texture_end = texture_start.saturating_add(texture_slot_count);
        if texture_end > descriptor_heap_plan.resource_descriptor_kinds.len() {
            continue;
        }
        if !effect_heap_slice_has_uniform_buffers(
            descriptor_heap_plan,
            base_resource_descriptor_index,
            uniform_buffer_count,
        ) {
            continue;
        }
        if !effect_heap_slice_has_sampled_images(
            descriptor_heap_plan,
            texture_start,
            texture_slot_count,
        ) {
            continue;
        }
        let sampled_images_before_slice = descriptor_heap_plan.resource_descriptor_kinds
            [..base_resource_descriptor_index]
            .iter()
            .filter(|kind| {
                **kind == NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage
            })
            .count();
        return Ok(Some(SceneEffectResourceHeapTextureLayout {
            base_resource_descriptor_index,
            base_sampler_descriptor_index: sampled_images_before_slice,
            uniform_buffer_count,
        }));
    }
    Err(format!(
        "scene effect pipeline shader '{}' requires a heap slice shaped as {} uniform buffers plus {} sampled textures",
        key.shader, uniform_buffer_count, texture_slot_count
    ))
}

fn effect_heap_slice_has_uniform_buffers(
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    first_resource_descriptor_index: usize,
    uniform_buffer_count: usize,
) -> bool {
    descriptor_heap_plan
        .resource_descriptor_kinds
        .iter()
        .skip(first_resource_descriptor_index)
        .take(uniform_buffer_count)
        .count()
        == uniform_buffer_count
        && descriptor_heap_plan
            .resource_descriptor_kinds
            .iter()
            .skip(first_resource_descriptor_index)
            .take(uniform_buffer_count)
            .all(|kind| {
                *kind == NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer
            })
}

fn scene_effect_descriptor_heap_stage_mappings(
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    key: &NativeVulkanSceneEffectPipelineKey<'_>,
    shaders: NativeVulkanSceneEffectPipelineShaders<'_>,
) -> Result<SceneEffectDescriptorHeapStageMappings, String> {
    let vertex_reflection =
        native_vulkan_reflect_scene_spirv_resources(shaders.vertex_spirv, "scene effect vertex")?;
    let fragment_reflection = native_vulkan_reflect_scene_spirv_resources(
        shaders.fragment_spirv,
        "scene effect fragment",
    )?;
    validate_scene_effect_reflected_texture_slots(key, &vertex_reflection, &fragment_reflection)?;
    let Some(layout) = scene_effect_resource_heap_texture_layout(descriptor_heap_plan, key)? else {
        return Ok(SceneEffectDescriptorHeapStageMappings {
            vertex: Vec::new(),
            fragment: Vec::new(),
        });
    };
    if key.effect_uniform_buffer_count > 0 && layout.uniform_buffer_count == 0 {
        return Err(format!(
            "scene effect pipeline shader '{}' requires reflected effect uniform mapping but heap layout has no leading uniform",
            key.shader
        ));
    }
    if key.effect_uniform_buffer_count > 0
        && vertex_reflection.uniform_buffer_bindings.is_empty()
        && fragment_reflection.uniform_buffer_bindings.is_empty()
    {
        return Err(format!(
            "scene effect pipeline shader '{}' requires effect uniform mapping but SPIR-V reflection found no uniform-buffer bindings",
            key.shader
        ));
    }
    let mut vertex = scene_effect_uniform_descriptor_heap_mappings(
        descriptor_heap_plan,
        key,
        &layout,
        &vertex_reflection.uniform_buffer_bindings,
        vk::ShaderStageFlags::VERTEX,
    )?;
    vertex.extend(scene_effect_descriptor_heap_mappings_for_reflected_slots(
        descriptor_heap_plan,
        key,
        &layout,
        &vertex_reflection.sampled_image_bindings,
    )?);
    let mut fragment = scene_effect_uniform_descriptor_heap_mappings(
        descriptor_heap_plan,
        key,
        &layout,
        &fragment_reflection.uniform_buffer_bindings,
        vk::ShaderStageFlags::FRAGMENT,
    )?;
    fragment.extend(scene_effect_descriptor_heap_mappings_for_reflected_slots(
        descriptor_heap_plan,
        key,
        &layout,
        &fragment_reflection.sampled_image_bindings,
    )?);
    Ok(SceneEffectDescriptorHeapStageMappings { vertex, fragment })
}

fn scene_effect_uniform_descriptor_heap_mappings(
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    key: &NativeVulkanSceneEffectPipelineKey<'_>,
    layout: &SceneEffectResourceHeapTextureLayout,
    uniform_bindings: &BTreeSet<u32>,
    stage: vk::ShaderStageFlags,
) -> Result<Vec<NativeVulkanDescriptorHeapShaderBindingMapping>, String> {
    if uniform_bindings.is_empty() {
        return Ok(Vec::new());
    }
    if key.effect_uniform_buffer_count == 0 {
        return Err(format!(
            "scene effect pipeline shader '{}' SPIR-V reflected uniform-buffer bindings {:?} but the effect heap slice is texture-only",
            key.shader, uniform_bindings
        ));
    }
    uniform_bindings
        .iter()
        .map(|binding| {
            let resource_descriptor_index =
                effect_uniform_resource_descriptor_index_for_stage(key, layout, stage)?;
            native_vulkan_vulkanalia_descriptor_heap_resource_relative_uniform_buffer_binding_mapping(
                descriptor_heap_plan,
                *binding,
                layout.base_resource_descriptor_index,
                resource_descriptor_index,
            )
        })
        .collect()
}

fn effect_uniform_resource_descriptor_index_for_stage(
    key: &NativeVulkanSceneEffectPipelineKey<'_>,
    layout: &SceneEffectResourceHeapTextureLayout,
    stage: vk::ShaderStageFlags,
) -> Result<usize, String> {
    if key.shader == "effects/iris" && key.effect_uniform_buffer_count == 2 {
        if stage == vk::ShaderStageFlags::VERTEX {
            return Ok(layout.base_resource_descriptor_index);
        }
        if stage == vk::ShaderStageFlags::FRAGMENT {
            return Ok(layout.base_resource_descriptor_index + 1);
        }
    }
    if key.effect_uniform_buffer_count == 1 {
        return Ok(layout.base_resource_descriptor_index);
    }
    Err(format!(
        "scene effect pipeline shader '{}' has unsupported stage uniform layout: {} uniform buffers for {:?}",
        key.shader, key.effect_uniform_buffer_count, stage
    ))
}

fn scene_effect_descriptor_heap_mappings_for_reflected_slots(
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    key: &NativeVulkanSceneEffectPipelineKey<'_>,
    layout: &SceneEffectResourceHeapTextureLayout,
    reflected_texture_slots: &BTreeSet<u32>,
) -> Result<Vec<NativeVulkanDescriptorHeapShaderBindingMapping>, String> {
    let texture_slots = scene_effect_texture_slots(key.texture_slot_mask);
    reflected_texture_slots
        .iter()
        .map(|slot| {
            let ordinal = texture_slots
                .iter()
                .position(|texture_slot| texture_slot == slot)
                .ok_or_else(|| {
                    format!(
                        "scene effect pipeline shader '{}' reflected sampled-image binding {} is not present in WE texture slot mask {:#x}",
                        key.shader, slot, key.texture_slot_mask
                    )
                })?;
            if layout.uniform_buffer_count > 0 {
                native_vulkan_vulkanalia_descriptor_heap_resource_relative_combined_image_sampler_binding_mapping(
                    descriptor_heap_plan,
                    *slot,
                    layout.base_resource_descriptor_index,
                    layout.base_resource_descriptor_index + layout.uniform_buffer_count + ordinal,
                    layout.base_sampler_descriptor_index,
                    layout.base_sampler_descriptor_index + ordinal,
                )
            } else {
                native_vulkan_vulkanalia_descriptor_heap_resource_relative_sampled_image_binding_mapping(
                    descriptor_heap_plan,
                    *slot,
                    layout.base_resource_descriptor_index,
                    layout.base_resource_descriptor_index + ordinal,
                    layout.base_sampler_descriptor_index,
                    layout.base_sampler_descriptor_index + ordinal,
                )
            }
        })
        .collect()
}

fn validate_scene_effect_reflected_texture_slots(
    key: &NativeVulkanSceneEffectPipelineKey<'_>,
    vertex: &NativeVulkanSceneSpirvResourceReflection,
    fragment: &NativeVulkanSceneSpirvResourceReflection,
) -> Result<(), String> {
    let expected = scene_effect_texture_slots(key.texture_slot_mask)
        .into_iter()
        .collect::<BTreeSet<_>>();
    let reflected = vertex
        .sampled_image_bindings
        .union(&fragment.sampled_image_bindings)
        .copied()
        .collect::<BTreeSet<_>>();
    if expected == reflected {
        return Ok(());
    }
    let missing = expected.difference(&reflected).copied().collect::<Vec<_>>();
    let unexpected = reflected.difference(&expected).copied().collect::<Vec<_>>();
    Err(format!(
        "scene effect pipeline shader '{}' sampled-image bindings do not match WE texture slot mask {:#x}: missing {:?}, unexpected {:?}",
        key.shader, key.texture_slot_mask, missing, unexpected
    ))
}

fn effect_heap_slice_has_sampled_images(
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    first_resource_descriptor_index: usize,
    texture_slot_count: usize,
) -> bool {
    descriptor_heap_plan
        .resource_descriptor_kinds
        .iter()
        .skip(first_resource_descriptor_index)
        .take(texture_slot_count)
        .count()
        == texture_slot_count
        && descriptor_heap_plan
            .resource_descriptor_kinds
            .iter()
            .skip(first_resource_descriptor_index)
            .take(texture_slot_count)
            .all(|kind| {
                *kind == NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage
            })
}

fn scene_effect_shader_resource_mapping_labels(
    key: &NativeVulkanSceneEffectPipelineKey<'_>,
) -> Vec<String> {
    let mut labels = (0..key.effect_uniform_buffer_count)
        .map(|ordinal| {
            if key.shader == "effects/iris" {
                match ordinal {
                    0 => {
                        "VK_EXT_descriptor_heap effects/iris VS slot2 uniform -> effect-heap-slice-offset0"
                            .to_owned()
                    }
                    1 => {
                        "VK_EXT_descriptor_heap effects/iris PS slot3 uniform -> effect-heap-slice-offset1"
                            .to_owned()
                    }
                    _ => format!(
                        "VK_EXT_descriptor_heap effects/iris extra uniform ordinal{ordinal} -> effect-heap-slice-offset{ordinal}"
                    ),
                }
            } else {
                format!(
                    "VK_EXT_descriptor_heap effect.uniform_ordinal{ordinal} -> effect-heap-slice-offset{ordinal}"
                )
            }
        })
        .collect::<Vec<_>>();
    labels.extend(scene_effect_texture_slots(key.texture_slot_mask)
        .into_iter()
        .enumerate()
        .map(|(ordinal, slot)| {
            format!(
                "VK_EXT_descriptor_heap we.texture_slot{slot}.g_Texture{slot} -> effect-heap-slice-offset{}",
                key.effect_uniform_buffer_count.saturating_add(ordinal)
            )
        }));
    labels
}

fn scene_effect_shader_combo_labels(key: &NativeVulkanSceneEffectPipelineKey<'_>) -> Vec<String> {
    key.shader_combo_values
        .iter()
        .map(|combo| format!("{}={}", combo.name, combo.value))
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
        SceneAlphaWriteMode::Default => "default-inherited-rgba",
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
                "VK_EXT_descriptor_heap effects/iris VS slot2 uniform -> effect-heap-slice-offset0"
                    .to_owned(),
                "VK_EXT_descriptor_heap effects/iris PS slot3 uniform -> effect-heap-slice-offset1"
                    .to_owned(),
                "VK_EXT_descriptor_heap we.texture_slot0.g_Texture0 -> effect-heap-slice-offset2"
                    .to_owned()
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
    fn effect_pipeline_plan_rejects_undeclared_iris_combo() {
        let mut key = effect_key();
        key.shader_combo_values.push(
            super::super::pipeline::NativeVulkanScenePipelineShaderComboValue {
                name: "NOT_IRIS".to_owned(),
                value: 1,
            },
        );

        let err = native_vulkan_scene_effect_pipeline_create_plan(&key)
            .expect_err("undeclared iris combo must fail");

        assert!(err.contains("undeclared WE combo 'NOT_IRIS'"));
    }

    #[test]
    fn effect_pipeline_plan_rejects_undeclared_iris_texture_slot() {
        let mut key = effect_key();
        key.texture_slot_mask = 0b101;

        let err = native_vulkan_scene_effect_pipeline_create_plan(&key)
            .expect_err("undeclared iris texture slot must fail");

        assert!(err.contains("outside shader interface"));
        assert!(err.contains("0x00000004"));
    }

    #[test]
    fn effect_pipeline_descriptor_heap_layout_accepts_sampled_image_set() {
        let descriptor_heap_plan = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: vec![
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                ],
                sampler_count: 1,
                properties: descriptor_properties(),
            },
        );

        let key = texture_only_effect_key();
        let layout = scene_effect_resource_heap_texture_layout(&descriptor_heap_plan, &key)
            .expect("effect descriptor heap layout")
            .expect("texture layout");

        assert_eq!(layout.base_resource_descriptor_index, 0);
        assert_eq!(layout.base_sampler_descriptor_index, 0);
        assert_eq!(layout.uniform_buffer_count, 0);
    }

    #[test]
    fn effect_pipeline_descriptor_heap_layout_accepts_effect_uniform_then_sampled_images() {
        let descriptor_heap_plan = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: vec![
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                ],
                sampler_count: 1,
                properties: descriptor_properties(),
            },
        );

        let key = uniform_effect_key();
        let layout = scene_effect_resource_heap_texture_layout(&descriptor_heap_plan, &key)
            .expect("effect descriptor heap layout")
            .expect("texture layout");

        assert_eq!(layout.base_resource_descriptor_index, 0);
        assert_eq!(layout.base_sampler_descriptor_index, 0);
        assert_eq!(layout.uniform_buffer_count, 2);
    }

    #[test]
    fn effect_pipeline_descriptor_heap_layout_accepts_uniform_only_slice() {
        let descriptor_heap_plan = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: vec![
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                ],
                sampler_count: 0,
                properties: descriptor_properties(),
            },
        );

        let mut key = uniform_effect_key();
        key.texture_slot_mask = 0;
        let layout = scene_effect_resource_heap_texture_layout(&descriptor_heap_plan, &key)
            .expect("effect descriptor heap layout")
            .expect("uniform layout");

        assert_eq!(layout.base_resource_descriptor_index, 0);
        assert_eq!(layout.uniform_buffer_count, 2);
    }

    #[test]
    fn effect_pipeline_descriptor_heap_layout_rejects_non_texture_tail() {
        let descriptor_heap_plan = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: vec![
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                ],
                sampler_count: 0,
                properties: descriptor_properties(),
            },
        );

        let mut key = effect_key();
        key.effect_uniform_buffer_count = 2;
        let err = scene_effect_resource_heap_texture_layout(&descriptor_heap_plan, &key)
            .expect_err("non-texture tail must fail");

        assert!(err.contains("requires 1 WE texture mappings"));
    }

    #[test]
    fn effect_pipeline_stage_mappings_include_reflected_uniform_bindings() {
        let descriptor_heap_plan = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: vec![
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                ],
                sampler_count: 1,
                properties: descriptor_properties(),
            },
        );
        let key = uniform_effect_key();
        let vertex_spirv = spirv_with_uniform_binding(7);
        let fragment_spirv = spirv_with_uniform_and_sampled_bindings(8, &[0]);

        let mappings = scene_effect_descriptor_heap_stage_mappings(
            &descriptor_heap_plan,
            &key,
            NativeVulkanSceneEffectPipelineShaders {
                vertex_spirv: &vertex_spirv,
                fragment_spirv: &fragment_spirv,
            },
        )
        .expect("stage mappings");

        assert_eq!(mappings.vertex.len(), 1);
        assert_eq!(mappings.vertex[0].first_binding, 7);
        assert_eq!(
            mappings.vertex[0].resource_mask,
            vk::SpirvResourceTypeFlagsEXT::UNIFORM_BUFFER
        );
        assert_eq!(mappings.fragment.len(), 2);
        assert_eq!(mappings.fragment[0].first_binding, 8);
        assert_eq!(
            mappings.fragment[0].resource_mask,
            vk::SpirvResourceTypeFlagsEXT::UNIFORM_BUFFER
        );
        assert_eq!(mappings.fragment[1].first_binding, 0);
        unsafe {
            assert_eq!(
                mappings.vertex[0].source_data.constant_offset.heap_offset,
                0
            );
            assert_eq!(
                mappings.fragment[0].source_data.constant_offset.heap_offset,
                32
            );
            assert!(mappings.fragment[1].source_data.constant_offset.heap_offset > 0);
        }
    }

    #[test]
    fn effect_pipeline_stage_mappings_follow_reflected_texture_stages() {
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
        let key = masked_texture_only_effect_key();
        let vertex_spirv = spirv_with_sampled_image_bindings(&[0]);
        let fragment_spirv = spirv_with_sampled_image_bindings(&[1]);

        let mappings = scene_effect_descriptor_heap_stage_mappings(
            &descriptor_heap_plan,
            &key,
            NativeVulkanSceneEffectPipelineShaders {
                vertex_spirv: &vertex_spirv,
                fragment_spirv: &fragment_spirv,
            },
        )
        .expect("stage mappings");

        assert_eq!(mappings.vertex.len(), 1);
        assert_eq!(mappings.vertex[0].first_binding, 0);
        assert_eq!(
            mappings.vertex[0].resource_mask,
            vk::SpirvResourceTypeFlagsEXT::COMBINED_SAMPLED_IMAGE
        );
        assert_eq!(mappings.fragment.len(), 1);
        assert_eq!(mappings.fragment[0].first_binding, 1);
        assert_eq!(
            mappings.fragment[0].resource_mask,
            vk::SpirvResourceTypeFlagsEXT::COMBINED_SAMPLED_IMAGE
        );
    }

    #[test]
    fn effect_pipeline_stage_mappings_accept_uniform_only_pass() {
        let descriptor_heap_plan = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: vec![
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                ],
                sampler_count: 0,
                properties: descriptor_properties(),
            },
        );
        let mut key = uniform_effect_key();
        key.texture_slot_mask = 0;
        let vertex_spirv = spirv_with_uniform_binding(7);
        let fragment_spirv = spirv_words(Vec::new());

        let mappings = scene_effect_descriptor_heap_stage_mappings(
            &descriptor_heap_plan,
            &key,
            NativeVulkanSceneEffectPipelineShaders {
                vertex_spirv: &vertex_spirv,
                fragment_spirv: &fragment_spirv,
            },
        )
        .expect("stage mappings");

        assert_eq!(mappings.vertex.len(), 1);
        assert_eq!(mappings.vertex[0].first_binding, 7);
        assert!(mappings.fragment.is_empty());
    }

    #[test]
    fn effect_pipeline_stage_mappings_reject_texture_slot_reflection_mismatch() {
        let descriptor_heap_plan = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: vec![
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                ],
                sampler_count: 1,
                properties: descriptor_properties(),
            },
        );
        let key = texture_only_effect_key();
        let fragment_spirv = spirv_with_sampled_image_bindings(&[1]);

        let err = scene_effect_descriptor_heap_stage_mappings(
            &descriptor_heap_plan,
            &key,
            NativeVulkanSceneEffectPipelineShaders {
                vertex_spirv: &spirv_words(Vec::new()),
                fragment_spirv: &fragment_spirv,
            },
        )
        .expect_err("sampled image binding mismatch must fail");

        assert!(err.contains("sampled-image bindings do not match"));
        assert!(err.contains("missing [0]"));
        assert!(err.contains("unexpected [1]"));
    }

    #[test]
    fn effect_pipeline_stage_mappings_reject_missing_reflected_uniform() {
        let descriptor_heap_plan = native_vulkan_vulkanalia_descriptor_heap_resource_plan(
            NativeVulkanVulkanaliaDescriptorHeapResourcePlanInput {
                resource_descriptors: vec![
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::UniformBuffer,
                    NativeVulkanVulkanaliaDescriptorHeapResourceDescriptorKind::SampledImage,
                ],
                sampler_count: 1,
                properties: descriptor_properties(),
            },
        );
        let key = uniform_effect_key();
        let vertex_spirv = spirv_words(Vec::new());
        let fragment_spirv = spirv_with_sampled_image_bindings(&[0]);

        let err = scene_effect_descriptor_heap_stage_mappings(
            &descriptor_heap_plan,
            &key,
            NativeVulkanSceneEffectPipelineShaders {
                vertex_spirv: &vertex_spirv,
                fragment_spirv: &fragment_spirv,
            },
        )
        .expect_err("missing reflected uniform must fail");

        assert!(err.contains("found no uniform-buffer bindings"));
    }

    fn effect_key() -> NativeVulkanSceneEffectPipelineKey<'static> {
        NativeVulkanSceneEffectPipelineKey {
            shader: "effects/iris",
            shader_combo_values: Vec::new(),
            effect: WeEffectKind::Iris,
            blend: SceneEffectPassBlend::NormalReplace,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            alpha_write: SceneAlphaWriteMode::Default,
            target_format: vk::Format::R16G16B16A16_SFLOAT,
            texture_slot_mask: 0b1,
            effect_uniform_buffer_count: 2,
            raster_geometry: super::super::effect_pipeline::NativeVulkanSceneEffectRasterGeometry::FullscreenTriangle,
        }
    }

    fn masked_effect_key() -> NativeVulkanSceneEffectPipelineKey<'static> {
        let mut key = effect_key();
        key.texture_slot_mask = 0b11;
        key.shader_combo_values = vec![
            super::super::pipeline::NativeVulkanScenePipelineShaderComboValue {
                name: "MASK".to_owned(),
                value: 1,
            },
        ];
        key
    }

    fn texture_only_effect_key() -> NativeVulkanSceneEffectPipelineKey<'static> {
        let mut key = effect_key();
        key.shader = "effects/blur_downsample4";
        key.effect = WeEffectKind::Unknown;
        key.effect_uniform_buffer_count = 0;
        key
    }

    fn masked_texture_only_effect_key() -> NativeVulkanSceneEffectPipelineKey<'static> {
        let mut key = texture_only_effect_key();
        key.texture_slot_mask = 0b11;
        key.effect_uniform_buffer_count = 0;
        key
    }

    fn uniform_effect_key() -> NativeVulkanSceneEffectPipelineKey<'static> {
        let mut key = effect_key();
        key.effect_uniform_buffer_count = 2;
        key
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

    fn spirv_with_uniform_binding(binding: u32) -> Vec<u32> {
        spirv_with_uniform_and_sampled_bindings(binding, &[])
    }

    fn spirv_with_uniform_and_sampled_bindings(
        uniform_binding: u32,
        sampled_image_bindings: &[u32],
    ) -> Vec<u32> {
        let mut instructions = sampled_image_instructions(sampled_image_bindings);
        instructions.extend([
            instr(71, &[7, 33, uniform_binding]),
            instr(71, &[7, 34, 0]),
            instr(59, &[30, 7, 2]),
        ]);
        spirv_words(instructions)
    }

    fn spirv_with_sampled_image_bindings(bindings: &[u32]) -> Vec<u32> {
        spirv_words(sampled_image_instructions(bindings))
    }

    fn sampled_image_instructions(bindings: &[u32]) -> Vec<Vec<u32>> {
        let mut instructions = Vec::new();
        if bindings.is_empty() {
            return instructions;
        }
        instructions.push(instr(25, &[3, 1, 2, 0, 0, 0, 1]));
        instructions.push(instr(27, &[4, 3]));
        instructions.push(instr(32, &[5, 0, 4]));
        for (index, binding) in bindings.iter().enumerate() {
            let variable = 20 + u32::try_from(index).unwrap_or(u32::MAX);
            instructions.push(instr(71, &[variable, 33, *binding]));
            instructions.push(instr(71, &[variable, 34, 0]));
            instructions.push(instr(59, &[5, variable, 0]));
        }
        instructions
    }

    fn spirv_words(instructions: Vec<Vec<u32>>) -> Vec<u32> {
        let mut words = vec![0x0723_0203, 0x0001_0000, 0, 32, 0];
        for instruction in instructions {
            words.extend(instruction);
        }
        words
    }

    fn instr(opcode: u16, operands: &[u32]) -> Vec<u32> {
        let mut words = vec![((operands.len() as u32 + 1) << 16) | u32::from(opcode)];
        words.extend_from_slice(operands);
        words
    }
}
