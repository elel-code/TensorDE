//! Scene Vulkan pipeline selection for mesh and fullscreen effect draws.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/effect-format.md`
//! - `references/godot/servers/rendering/renderer_rd/pipeline_hash_map_rd.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.*`

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use crate::engine::scene::{
    SceneCompositeBlend, ScenePipelineBlend, SceneRenderPassRecord, SceneRenderTargetKind,
    SceneRenderingDeviceGraphPlan, SceneStorage, SceneStringId,
};
use crate::renderer::native_vulkan::scene::native_vulkan_scene_shader_for_key;
use crate::renderer::native_vulkan::{
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_combined_image_sampler_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_storage_buffer_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_uniform_buffer_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping_info,
};

use super::effect_target::SceneEffectTargetImagePlan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct ScenePipelineDescriptorLayout {
    pub sampled_slots: Vec<u32>,
    pub material_uniform_enabled: bool,
    pub skinning_storage_enabled: bool,
}

pub(in crate::renderer::native_vulkan) struct ScenePipelineResources {
    pub entries: Vec<ScenePipelineEntry>,
}

pub(in crate::renderer::native_vulkan) struct ScenePipelineEntry {
    key: ScenePipelineKey,
    pub pipeline: vk::Pipeline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScenePipelineKey {
    shader_key: SceneStringId,
    blend: SceneGpuBlend,
    target_format: vk::Format,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SceneGpuBlend {
    Disabled,
    Alpha,
    Additive,
    AlphaToCoverage,
    Multiply,
    Screen,
    Maximum,
    Modulate,
    HslColor,
}

impl SceneGpuBlend {
    const fn requires_advanced_operation(self) -> bool {
        matches!(self, Self::Multiply | Self::Screen | Self::HslColor)
    }
}

pub(in crate::renderer::native_vulkan) fn scene_pipeline_descriptor_layout(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
) -> Result<ScenePipelineDescriptorLayout, String> {
    let mut texture_slot_mask = 0u32;
    let mut material_uniform_enabled = false;
    for key in drawn_pass_material_keys(storage, graph)? {
        let shader_key = storage
            .string(key.shader_key)
            .ok_or_else(|| "scene drawable pass has no shader key".to_owned())?;
        let contract = storage
            .shader_contracts()
            .iter()
            .find(|contract| contract.shader_key == key.shader_key)
            .ok_or_else(|| format!("scene shader {shader_key:?} has no shader contract"))?;
        let shader = native_vulkan_scene_shader_for_key(shader_key)
            .ok_or_else(|| format!("scene shader {shader_key:?} is not built into the catalog"))?;
        texture_slot_mask |= contract.texture_slot_mask;
        material_uniform_enabled |= shader.parameter_layout.uses_material_uniform();
    }
    Ok(ScenePipelineDescriptorLayout {
        sampled_slots: sampled_slots(texture_slot_mask),
        material_uniform_enabled,
        skinning_storage_enabled: !graph.puppet_bone_matrices.is_empty(),
    })
}

pub(in crate::renderer::native_vulkan) fn scene_pipeline_indices_for_draws(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    swapchain_format: vk::Format,
    effect_target_plans: &[SceneEffectTargetImagePlan],
) -> Result<Vec<u32>, String> {
    let keys = drawn_pass_pipeline_keys(storage, graph, swapchain_format, effect_target_plans)?;
    let mut indices = vec![0u32; graph.mesh_draws.len()];
    for pass in graph.pass_nodes.iter().filter(|pass| pass.mesh_draw_count != 0) {
        let pass_record = storage
            .document()
            .render_passes
            .get(pass.pass_record_index as usize)
            .ok_or_else(|| "scene drawable pass references a missing pass record".to_owned())?;
        let key = ScenePipelineKey {
            shader_key: pass_record.shader_key,
            blend: scene_gpu_blend(pass_record, pass.target),
            target_format: pass_target_format(
                graph,
                pass,
                swapchain_format,
                effect_target_plans,
            )?,
        };
        let pipeline_index = keys
            .iter()
            .position(|candidate| *candidate == key)
            .ok_or_else(|| "scene drawable pass has no pipeline key".to_owned())?
            as u32;
        let start = pass.mesh_draw_start as usize;
        let end = start.saturating_add(pass.mesh_draw_count as usize);
        for slot in indices.get_mut(start..end).unwrap_or(&mut []) {
            *slot = pipeline_index;
        }
    }
    Ok(indices)
}

pub(in crate::renderer::native_vulkan) fn create_scene_pipelines(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    descriptor_layout: &ScenePipelineDescriptorLayout,
    effect_target_plans: &[SceneEffectTargetImagePlan],
    advanced_blend_enabled: bool,
    advanced_blend_coherent: bool,
) -> Result<ScenePipelineResources, String> {
    let keys = drawn_pass_pipeline_keys(storage, graph, target_format, effect_target_plans)?;
    if keys.is_empty() {
        return Err("scene present requires at least one drawable pass pipeline".to_owned());
    }
    if keys
        .iter()
        .any(|key| key.blend.requires_advanced_operation())
        && (!advanced_blend_enabled || !advanced_blend_coherent)
    {
        return Err(
            "scene composite blend requires coherent VK_EXT_blend_operation_advanced support"
                .to_owned(),
        );
    }
    let mut entries = Vec::with_capacity(keys.len());
    for key in keys {
        let shader_key = storage
            .string(key.shader_key)
            .ok_or_else(|| "scene drawable pass has no shader key".to_owned())?;
        let shader = native_vulkan_scene_shader_for_key(shader_key)
            .ok_or_else(|| format!("scene shader {shader_key:?} is not in the built-in catalog"))?;
        match create_scene_pipeline(
            device,
            key.target_format,
            extent,
            shader.vertex_spirv,
            shader.fragment_spirv,
            descriptor_heap_plan,
            descriptor_layout,
            key.blend,
        ) {
            Ok(pipeline) => entries.push(ScenePipelineEntry { key, pipeline }),
            Err(err) => {
                destroy_scene_pipelines(device, ScenePipelineResources { entries });
                return Err(err);
            }
        }
    }
    Ok(ScenePipelineResources { entries })
}

pub(in crate::renderer::native_vulkan) fn destroy_scene_pipelines(
    device: &Device,
    resources: ScenePipelineResources,
) {
    unsafe {
        for entry in resources.entries {
            device.destroy_pipeline(entry.pipeline, None);
        }
    }
}

fn drawn_pass_pipeline_keys(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    swapchain_format: vk::Format,
    effect_target_plans: &[SceneEffectTargetImagePlan],
) -> Result<Vec<ScenePipelineKey>, String> {
    let mut keys = Vec::<ScenePipelineKey>::new();
    for pass in graph.pass_nodes.iter().filter(|pass| pass.mesh_draw_count != 0) {
        let pass_record = storage
            .document()
            .render_passes
            .get(pass.pass_record_index as usize)
            .ok_or_else(|| "scene drawable pass references a missing pass record".to_owned())?;
        if pass_record.shader_key == SceneStringId::NONE {
            return Err("scene drawable pass has no shader key".to_owned());
        }
        let key = ScenePipelineKey {
            shader_key: pass_record.shader_key,
            blend: scene_gpu_blend(pass_record, pass.target),
            target_format: pass_target_format(
                graph,
                pass,
                swapchain_format,
                effect_target_plans,
            )?,
        };
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    Ok(keys)
}

fn drawn_pass_material_keys(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
) -> Result<Vec<ScenePipelineKey>, String> {
    let mut keys = Vec::<ScenePipelineKey>::new();
    for pass in graph.pass_nodes.iter().filter(|pass| pass.mesh_draw_count != 0) {
        let pass_record = storage
            .document()
            .render_passes
            .get(pass.pass_record_index as usize)
            .ok_or_else(|| "scene drawable pass references a missing pass record".to_owned())?;
        if pass_record.shader_key == SceneStringId::NONE {
            return Err("scene drawable pass has no shader key".to_owned());
        }
        let key = ScenePipelineKey {
            shader_key: pass_record.shader_key,
            blend: scene_gpu_blend(pass_record, pass.target),
            target_format: vk::Format::UNDEFINED,
        };
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    Ok(keys)
}

fn scene_gpu_blend(
    pass: &SceneRenderPassRecord,
    target: SceneRenderTargetKind,
) -> SceneGpuBlend {
    if !matches!(
        target,
        SceneRenderTargetKind::SceneColor | SceneRenderTargetKind::Swapchain
    ) {
        return pipeline_gpu_blend(pass.pipeline_blend);
    }
    match pass.scene_blend {
        SceneCompositeBlend::Alpha | SceneCompositeBlend::Normal => {
            pipeline_gpu_blend(pass.pipeline_blend)
        }
        SceneCompositeBlend::Additive => SceneGpuBlend::Additive,
        SceneCompositeBlend::Multiply => SceneGpuBlend::Multiply,
        SceneCompositeBlend::Screen => SceneGpuBlend::Screen,
        SceneCompositeBlend::Max => SceneGpuBlend::Maximum,
        SceneCompositeBlend::Modulate => SceneGpuBlend::Modulate,
        SceneCompositeBlend::HslColor => SceneGpuBlend::HslColor,
        SceneCompositeBlend::AlphaToCoverage => SceneGpuBlend::AlphaToCoverage,
    }
}

fn pipeline_gpu_blend(blend: ScenePipelineBlend) -> SceneGpuBlend {
    match blend {
        ScenePipelineBlend::Disabled => SceneGpuBlend::Disabled,
        ScenePipelineBlend::Additive => SceneGpuBlend::Additive,
        ScenePipelineBlend::AlphaToCoverage => SceneGpuBlend::AlphaToCoverage,
        ScenePipelineBlend::Normal | ScenePipelineBlend::Translucent => SceneGpuBlend::Alpha,
    }
}

fn pass_target_format(
    graph: &SceneRenderingDeviceGraphPlan,
    pass: &crate::engine::scene::SceneRenderingDevicePassNode,
    swapchain_format: vk::Format,
    effect_target_plans: &[SceneEffectTargetImagePlan],
) -> Result<vk::Format, String> {
    if matches!(
        pass.target,
        crate::engine::scene::SceneRenderTargetKind::SceneColor
            | crate::engine::scene::SceneRenderTargetKind::Swapchain
    ) {
        return Ok(swapchain_format);
    }
    let allocation = graph
        .target_allocations
        .iter()
        .find(|allocation| {
            allocation.graph_index == pass.graph_index
                && allocation.target == pass.target
                && allocation.target_name == pass.target_name
        })
        .ok_or_else(|| {
            format!(
                "scene drawable pass target {:?}:{:?} has no graph allocation",
                pass.target, pass.target_name
            )
        })?;
    effect_target_plans
        .iter()
        .find(|plan| plan.physical_slot == allocation.physical_slot)
        .map(|plan| plan.format)
        .ok_or_else(|| {
            format!(
                "scene drawable pass target physical slot {} has no image plan",
                allocation.physical_slot
            )
        })
}

fn create_scene_pipeline(
    device: &Device,
    target_format: vk::Format,
    extent: vk::Extent2D,
    vertex_spirv: &[u32],
    fragment_spirv: &[u32],
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    descriptor_layout: &ScenePipelineDescriptorLayout,
    blend: SceneGpuBlend,
) -> Result<vk::Pipeline, String> {
    if extent.width == 0 || extent.height == 0 {
        return Err("scene pipeline requires non-zero extent".to_owned());
    }
    let vertex_module = create_shader_module(device, vertex_spirv, "scene vertex")?;
    let result = (|| -> Result<vk::Pipeline, String> {
        let fragment_module = create_shader_module(device, fragment_spirv, "scene fragment")?;
        let result = create_scene_pipeline_with_modules(
            device,
            target_format,
            vertex_module,
            fragment_module,
            descriptor_heap_plan,
            descriptor_layout,
            blend,
        );
        unsafe {
            device.destroy_shader_module(fragment_module, None);
        }
        result
    })();
    unsafe {
        device.destroy_shader_module(vertex_module, None);
    }
    result
}

fn create_scene_pipeline_with_modules(
    device: &Device,
    target_format: vk::Format,
    vertex_module: vk::ShaderModule,
    fragment_module: vk::ShaderModule,
    descriptor_heap_plan: &NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    descriptor_layout: &ScenePipelineDescriptorLayout,
    blend: SceneGpuBlend,
) -> Result<vk::Pipeline, String> {
    let shader_entry = b"main\0";
    let mut vertex_mappings = vec![
        native_vulkan_vulkanalia_descriptor_heap_resource_relative_uniform_buffer_binding_mapping(
            descriptor_heap_plan,
            2,
            0,
            0,
        )?,
    ];
    let skinning_descriptor_index = 1 + usize::from(descriptor_layout.material_uniform_enabled);
    if descriptor_layout.skinning_storage_enabled {
        vertex_mappings.push(
            native_vulkan_vulkanalia_descriptor_heap_resource_relative_storage_buffer_binding_mapping(
                descriptor_heap_plan,
                4,
                0,
                skinning_descriptor_index,
            )?,
        );
    }
    let mut vertex_mapping_info =
        native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping_info(&vertex_mappings)?;
    let mut vertex_stage = vk::PipelineShaderStageCreateInfo::builder()
        .stage(vk::ShaderStageFlags::VERTEX)
        .module(vertex_module)
        .name(shader_entry)
        .build();
    vertex_stage.next = &mut vertex_mapping_info as *mut _ as *const std::ffi::c_void;

    let mut fragment_mappings = Vec::new();
    if descriptor_layout.material_uniform_enabled {
        fragment_mappings.push(
            native_vulkan_vulkanalia_descriptor_heap_resource_relative_uniform_buffer_binding_mapping(
                descriptor_heap_plan,
                3,
                0,
                1,
            )?,
        );
    }
    let sampled_base = 1
        + usize::from(descriptor_layout.material_uniform_enabled)
        + usize::from(descriptor_layout.skinning_storage_enabled);
    for (sampled_index, slot) in descriptor_layout.sampled_slots.iter().enumerate() {
        fragment_mappings.push(
            native_vulkan_vulkanalia_descriptor_heap_resource_relative_combined_image_sampler_binding_mapping(
                descriptor_heap_plan,
                *slot,
                0,
                sampled_base + sampled_index,
                0,
                sampled_index,
            )?,
        );
    }
    let mut fragment_mapping_info =
        native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping_info(&fragment_mappings)?;
    let mut fragment_stage = vk::PipelineShaderStageCreateInfo::builder()
        .stage(vk::ShaderStageFlags::FRAGMENT)
        .module(fragment_module)
        .name(shader_entry)
        .build();
    if !fragment_mappings.is_empty() {
        fragment_stage.next = &mut fragment_mapping_info as *mut _ as *const std::ffi::c_void;
    }
    create_graphics_pipeline(
        device,
        target_format,
        [vertex_stage, fragment_stage],
        blend,
    )
}

fn create_graphics_pipeline(
    device: &Device,
    target_format: vk::Format,
    stages: [vk::PipelineShaderStageCreateInfo; 2],
    blend: SceneGpuBlend,
) -> Result<vk::Pipeline, String> {
    let binding = vk::VertexInputBindingDescription::builder()
        .binding(0)
        .stride(super::SCENE_MESH_VERTEX_STRIDE_BYTES)
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
        vk::VertexInputAttributeDescription::builder()
            .location(3)
            .binding(0)
            .format(vk::Format::R32G32B32A32_UINT)
            .offset(20)
            .build(),
        vk::VertexInputAttributeDescription::builder()
            .location(4)
            .binding(0)
            .format(vk::Format::R32G32B32A32_SFLOAT)
            .offset(36)
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
    let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
        .viewport_count(1)
        .scissor_count(1)
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
        .alpha_to_coverage_enable(blend == SceneGpuBlend::AlphaToCoverage)
        .build();
    let color_attachment = scene_color_blend_attachment(blend);
    let color_attachments = [color_attachment];
    let mut advanced_blend = vk::PipelineColorBlendAdvancedStateCreateInfoEXT::builder()
        .src_premultiplied(false)
        .dst_premultiplied(false)
        .blend_overlap(vk::BlendOverlapEXT::UNCORRELATED)
        .build();
    let mut color_blend_builder =
        vk::PipelineColorBlendStateCreateInfo::builder().attachments(&color_attachments);
    if blend.requires_advanced_operation() {
        color_blend_builder = color_blend_builder.push_next(&mut advanced_blend);
    }
    let color_blend = color_blend_builder.build();
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
        .dynamic_state(&dynamic_state)
        .layout(vk::PipelineLayout::null())
        .render_pass(vk::RenderPass::null())
        .subpass(0)
        .push_next(&mut rendering_info);
    pipeline_info = pipeline_info.push_next(&mut pipeline_flags2);
    let pipeline_info = pipeline_info.build();
    let (pipelines, _success_code) = unsafe {
        device.create_graphics_pipelines(vk::PipelineCache::null(), &[pipeline_info], None)
    }
    .map_err(|err| format!("vkCreateGraphicsPipelines(vulkanalia scene): {err:?}"))?;
    Ok(pipelines[0])
}

fn create_shader_module(
    device: &Device,
    code: &[u32],
    label: &'static str,
) -> Result<vk::ShaderModule, String> {
    if code.first().copied() != Some(0x0723_0203) {
        return Err(format!("scene {label} shader is not valid SPIR-V bytecode"));
    }
    let create_info = vk::ShaderModuleCreateInfo::builder()
        .code(code)
        .code_size(std::mem::size_of_val(code));
    unsafe { device.create_shader_module(&create_info, None) }
        .map_err(|err| format!("vkCreateShaderModule(vulkanalia {label}): {err:?}"))
}

fn scene_color_blend_attachment(
    blend: SceneGpuBlend,
) -> vk::PipelineColorBlendAttachmentState {
    let builder = vk::PipelineColorBlendAttachmentState::builder().color_write_mask(
        vk::ColorComponentFlags::R
            | vk::ColorComponentFlags::G
            | vk::ColorComponentFlags::B
            | vk::ColorComponentFlags::A,
    );
    match blend {
        SceneGpuBlend::Disabled | SceneGpuBlend::AlphaToCoverage => {
            builder.blend_enable(false).build()
        }
        SceneGpuBlend::Additive => builder
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE)
            .alpha_blend_op(vk::BlendOp::ADD)
            .build(),
        SceneGpuBlend::Multiply => advanced_blend_attachment(builder, vk::BlendOp::MULTIPLY_EXT),
        SceneGpuBlend::Screen => advanced_blend_attachment(builder, vk::BlendOp::SCREEN_EXT),
        SceneGpuBlend::HslColor => {
            advanced_blend_attachment(builder, vk::BlendOp::HSL_COLOR_EXT)
        }
        SceneGpuBlend::Maximum => builder
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::ONE)
            .dst_color_blend_factor(vk::BlendFactor::ONE)
            .color_blend_op(vk::BlendOp::MAX)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE)
            .alpha_blend_op(vk::BlendOp::MAX)
            .build(),
        SceneGpuBlend::Modulate => builder
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::DST_COLOR)
            .dst_color_blend_factor(vk::BlendFactor::ONE)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ZERO)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE)
            .alpha_blend_op(vk::BlendOp::ADD)
            .build(),
        SceneGpuBlend::Alpha => builder
            .blend_enable(true)
            .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
            .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .color_blend_op(vk::BlendOp::ADD)
            .src_alpha_blend_factor(vk::BlendFactor::ONE)
            .dst_alpha_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
            .alpha_blend_op(vk::BlendOp::ADD)
            .build(),
    }
}

fn advanced_blend_attachment(
    builder: vk::PipelineColorBlendAttachmentStateBuilder,
    operation: vk::BlendOp,
) -> vk::PipelineColorBlendAttachmentState {
    builder
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::ONE)
        .dst_color_blend_factor(vk::BlendFactor::ZERO)
        .color_blend_op(operation)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
        .alpha_blend_op(operation)
        .build()
}

fn sampled_slots(mask: u32) -> Vec<u32> {
    (0..32).filter(|slot| mask & (1u32 << slot) != 0).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{
        SceneBinaryDocument, SceneRenderPassKind, SceneRenderPassRecord, SceneRenderTargetKind,
        SceneRenderingDeviceGraphPlan, SceneRenderingDeviceMeshDraw, SceneRenderingDevicePassNode,
        SceneRenderingDeviceTargetAllocation, SceneShaderContractRecord,
    };

    #[test]
    fn pipeline_indices_follow_drawn_pass_shader_and_blend_order() {
        let storage = SceneStorage::from_document(SceneBinaryDocument {
            strings: vec![
                "genericimage4".to_owned(),
                "effects/opacity__SLOTS_1".to_owned(),
                "generic-pipeline".to_owned(),
                "opacity-pipeline".to_owned(),
            ],
            shader_contracts: vec![
                SceneShaderContractRecord {
                    shader_key: SceneStringId(0),
                    pipeline_key: SceneStringId(2),
                    texture_slot_mask: 0b1,
                    constant_start: 0,
                    constant_count: 0,
                    resource_heap_count: 2,
                    sampler_heap_count: 1,
                },
                SceneShaderContractRecord {
                    shader_key: SceneStringId(1),
                    pipeline_key: SceneStringId(3),
                    texture_slot_mask: 0b1,
                    constant_start: 0,
                    constant_count: 0,
                    resource_heap_count: 1,
                    sampler_heap_count: 1,
                },
            ],
            render_passes: vec![
                render_pass(0, SceneStringId(0), ScenePipelineBlend::Normal),
                render_pass(1, SceneStringId(1), ScenePipelineBlend::Additive),
            ],
            ..SceneBinaryDocument::default()
        })
        .expect("storage");
        let graph = graph_with_passes(vec![pass_node(0, 0, 1), pass_node(1, 1, 1)]);

        let layout = scene_pipeline_descriptor_layout(&storage, &graph).expect("layout");
        let indices = scene_pipeline_indices_for_draws(
            &storage,
            &graph,
            vk::Format::B8G8R8A8_UNORM,
            &[],
        )
        .expect("indices");

        assert_eq!(layout.sampled_slots, vec![0]);
        assert!(layout.material_uniform_enabled);
        assert_eq!(indices, vec![0, 1]);
    }

    #[test]
    fn pipeline_indices_include_dynamic_rendering_target_format() {
        let storage = SceneStorage::from_document(SceneBinaryDocument {
            strings: vec!["effects/opacity__SLOTS_1".to_owned(), "pipeline".to_owned()],
            shader_contracts: vec![SceneShaderContractRecord {
                shader_key: SceneStringId(0),
                pipeline_key: SceneStringId(1),
                texture_slot_mask: 1,
                constant_start: 0,
                constant_count: 0,
                resource_heap_count: 1,
                sampler_heap_count: 1,
            }],
            render_passes: vec![
                render_pass(0, SceneStringId(0), ScenePipelineBlend::Normal),
                render_pass(1, SceneStringId(0), ScenePipelineBlend::Normal),
            ],
            ..SceneBinaryDocument::default()
        })
        .expect("storage");
        let mut offscreen_pass = pass_node(1, 1, 1);
        offscreen_pass.target = SceneRenderTargetKind::NamedFbo;
        offscreen_pass.target_name = SceneStringId(7);
        let mut graph = graph_with_passes(vec![pass_node(0, 0, 1), offscreen_pass]);
        graph.target_allocations = vec![SceneRenderingDeviceTargetAllocation {
            graph_index: 0,
            target: SceneRenderTargetKind::NamedFbo,
            target_name: SceneStringId(7),
            first_write_pass_id: 1,
            last_use_pass_id: 1,
            physical_slot: 3,
        }];
        let target_plans = vec![SceneEffectTargetImagePlan {
            physical_slot: 3,
            graph_index: 0,
            target: SceneRenderTargetKind::NamedFbo,
            target_name: SceneStringId(7),
            format: vk::Format::R16G16B16A16_SFLOAT,
            width: 960,
            height: 540,
            persistent_across_frames: true,
            aliased_logical_target_count: 1,
        }];

        let indices = scene_pipeline_indices_for_draws(
            &storage,
            &graph,
            vk::Format::B8G8R8A8_UNORM,
            &target_plans,
        )
        .expect("indices");

        assert_eq!(indices, vec![0, 1]);
    }

    #[test]
    fn final_target_pipeline_keys_include_scene_composite_blend() {
        let mut alpha = render_pass(0, SceneStringId(0), ScenePipelineBlend::Normal);
        alpha.scene_blend = SceneCompositeBlend::Alpha;
        let mut multiply = render_pass(1, SceneStringId(0), ScenePipelineBlend::Normal);
        multiply.scene_blend = SceneCompositeBlend::Multiply;
        let storage = SceneStorage::from_document(SceneBinaryDocument {
            strings: vec!["genericimage4".to_owned(), "pipeline".to_owned()],
            shader_contracts: vec![SceneShaderContractRecord {
                shader_key: SceneStringId(0),
                pipeline_key: SceneStringId(1),
                texture_slot_mask: 1,
                constant_start: 0,
                constant_count: 0,
                resource_heap_count: 2,
                sampler_heap_count: 1,
            }],
            render_passes: vec![alpha, multiply],
            ..SceneBinaryDocument::default()
        })
        .expect("storage");
        let graph = graph_with_passes(vec![pass_node(0, 0, 1), pass_node(1, 1, 1)]);

        let indices = scene_pipeline_indices_for_draws(
            &storage,
            &graph,
            vk::Format::B8G8R8A8_UNORM,
            &[],
        )
        .expect("indices");

        assert_eq!(indices, vec![0, 1]);
        assert_eq!(
            scene_gpu_blend(&storage.document().render_passes[1], SceneRenderTargetKind::SceneColor),
            SceneGpuBlend::Multiply
        );
        assert_eq!(
            scene_gpu_blend(&storage.document().render_passes[1], SceneRenderTargetKind::NamedFbo),
            SceneGpuBlend::Alpha
        );
    }

    #[test]
    fn gpu_blend_attachments_match_we_composite_equations() {
        let multiply = scene_color_blend_attachment(SceneGpuBlend::Multiply);
        assert_eq!(multiply.color_blend_op, vk::BlendOp::MULTIPLY_EXT);
        assert_eq!(multiply.alpha_blend_op, vk::BlendOp::MULTIPLY_EXT);

        let modulate = scene_color_blend_attachment(SceneGpuBlend::Modulate);
        assert_eq!(modulate.src_color_blend_factor, vk::BlendFactor::DST_COLOR);
        assert_eq!(modulate.dst_color_blend_factor, vk::BlendFactor::ONE);
        assert_eq!(modulate.color_blend_op, vk::BlendOp::ADD);
        assert_eq!(modulate.src_alpha_blend_factor, vk::BlendFactor::ZERO);
        assert_eq!(modulate.dst_alpha_blend_factor, vk::BlendFactor::ONE);
    }

    fn graph_with_passes(
        pass_nodes: Vec<SceneRenderingDevicePassNode>,
    ) -> SceneRenderingDeviceGraphPlan {
        SceneRenderingDeviceGraphPlan {
            pass_nodes,
            mesh_draws: vec![draw(), draw()],
            target_allocations: Vec::new(),
            sampled_bindings: Vec::new(),
            material_sampled_bindings: Vec::new(),
            puppet_bone_palettes: Vec::new(),
            puppet_bone_matrices: Vec::new(),
            resolved_object_count: 0,
            resolved_visible_object_count: 0,
            resolved_attachment_link_count: 0,
            resolved_visible_effect_instance_count: 0,
            resolved_visible_effect_pass_count: 0,
            resolved_visible_effect_fbo_count: 0,
            descriptor_heap_required: true,
            descriptor_heap_resource_count: 0,
            descriptor_heap_sampled_image_count: 0,
            descriptor_heap_uniform_buffer_count: 0,
            descriptor_heap_storage_buffer_count: 0,
            descriptor_heap_sampler_count: 0,
            graph_physical_target_count: 0,
            graph_aliased_target_count: 0,
            fifo_latest_ready_present_required: true,
        }
    }

    fn pass_node(
        pass_record_index: u32,
        mesh_draw_start: u32,
        mesh_draw_count: u32,
    ) -> SceneRenderingDevicePassNode {
        SceneRenderingDevicePassNode {
            graph_index: 0,
            pass_record_index,
            pass_id: pass_record_index,
            role: SceneRenderPassKind::EffectMaterial,
            target: SceneRenderTargetKind::SceneColor,
            target_name: SceneStringId::NONE,
            binding_start: 0,
            binding_count: 0,
            mesh_draw_start,
            mesh_draw_count,
        }
    }

    fn render_pass(
        id: u32,
        shader_key: SceneStringId,
        pipeline_blend: ScenePipelineBlend,
    ) -> SceneRenderPassRecord {
        SceneRenderPassRecord {
            id,
            role: SceneRenderPassKind::EffectMaterial,
            object: crate::engine::scene::SceneObjectHandle(
                crate::engine::scene::INVALID_OBJECT_ID,
            ),
            material: crate::engine::scene::SceneMaterialHandle(
                crate::engine::scene::INVALID_MATERIAL_ID,
            ),
            pass_index: id,
            shader_key,
            target: SceneRenderTargetKind::SceneColor,
            target_name: SceneStringId::NONE,
            binding_start: 0,
            binding_count: 0,
            pipeline_blend,
            scene_blend: crate::engine::scene::SceneCompositeBlend::Alpha,
            depth_test: crate::engine::scene::SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: crate::engine::scene::SceneCullMode::None,
        }
    }

    fn draw() -> SceneRenderingDeviceMeshDraw {
        SceneRenderingDeviceMeshDraw {
            primitive: crate::engine::scene::SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
            mesh_index: crate::engine::scene::INVALID_OBJECT_ID,
            resolved_object_index: crate::engine::scene::INVALID_OBJECT_ID,
            clip_transform: [[0.0; 4]; 4],
            skinning_palette_start: crate::engine::scene::INVALID_OBJECT_ID,
            skinning_palette_count: 0,
            resolved_color: crate::engine::scene::SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            resolved_alpha: 1.0,
            object: crate::engine::scene::SceneObjectHandle(
                crate::engine::scene::INVALID_OBJECT_ID,
            ),
            material: crate::engine::scene::SceneMaterialHandle(
                crate::engine::scene::INVALID_MATERIAL_ID,
            ),
            vertex_start: 0,
            vertex_count: 3,
            index_start: 0,
            index_count: 3,
        }
    }
}
