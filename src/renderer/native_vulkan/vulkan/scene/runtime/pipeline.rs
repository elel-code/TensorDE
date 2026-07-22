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
    SceneColorWriteMask, SceneCompositeBlend, SceneCullMode, ScenePipelineBlend,
    SceneRenderPassKind, SceneRenderPassRecord, SceneRenderTargetKind,
    SceneRenderingDeviceDrawPrimitive, SceneRenderingDeviceGraphPlan,
    SceneRenderingDevicePassNode, SceneStorage, SceneStringId,
};
use crate::renderer::native_vulkan::scene::{
    native_vulkan_scene_shader_for_key, native_vulkan_scene_vertex_spirv_for_primitive,
};
use crate::renderer::native_vulkan::{
    NativeVulkanVulkanaliaDescriptorHeapResourcePlanSnapshot,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_combined_image_sampler_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_mixed_input_attachment_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_storage_buffer_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_resource_relative_uniform_buffer_binding_mapping,
    native_vulkan_vulkanalia_descriptor_heap_shader_binding_mapping_info,
};

use super::effect_target::SceneEffectTargetImagePlan;
pub(in crate::renderer::native_vulkan) use super::descriptor_layout::{
    ScenePipelineDescriptorLayout,
};
use super::descriptor_layout::{
    ScenePipelineShaderDescriptorAccess, scene_passthrough_descriptor_access,
    scene_pipeline_shader_descriptor_access,
};

mod blend;
mod diagnostics;
mod particle_compute;
mod samples;
mod shader_module;

pub(in crate::renderer::native_vulkan) use diagnostics::emit_scene_pipeline_diagnostics_if_requested;
use blend::scene_color_blend_attachment;
use samples::ScenePipelineSamples;
use shader_module::create_shader_module;

pub(in crate::renderer::native_vulkan) struct ScenePipelineResources {
    pub entries: Vec<ScenePipelineEntry>,
    pub particle_compute: Option<vk::Pipeline>,
}

pub(in crate::renderer::native_vulkan) struct ScenePipelineEntry {
    key: ScenePipelineKey,
    pub pipeline: vk::Pipeline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScenePipelineKey {
    shader: ScenePipelineShader,
    primitive: SceneRenderingDeviceDrawPrimitive,
    blend: SceneGpuBlend,
    cull_mode: SceneCullMode,
    color_write_mask: SceneColorWriteMask,
    advanced_source_premultiplied: bool,
    advanced_blend_overlap: vk::BlendOverlapEXT,
    target_format: vk::Format,
    samples: ScenePipelineSamples,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScenePipelineShader {
    Authored(SceneStringId),
    EffectPassthrough(SceneStringId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SceneGpuBlend {
    Replace,
    Alpha,
    Additive,
    AlphaToCoverage,
    Multiply,
    MultiplyPremultiplied,
    Screen,
    ScreenPremultiplied,
    Maximum,
    Modulate,
    HslColor,
}

impl SceneGpuBlend {
    const fn requires_advanced_operation(self) -> bool {
        matches!(self, Self::Multiply | Self::Screen | Self::HslColor)
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Replace => "replace",
            Self::Alpha => "alpha",
            Self::Additive => "additive",
            Self::AlphaToCoverage => "alpha-to-coverage",
            Self::Multiply => "multiply",
            Self::MultiplyPremultiplied => "multiply-premultiplied-standard",
            Self::Screen => "screen",
            Self::ScreenPremultiplied => "screen-premultiplied-standard",
            Self::Maximum => "maximum",
            Self::Modulate => "modulate",
            Self::HslColor => "hsl-color",
        }
    }
}

pub(in crate::renderer::native_vulkan) fn scene_pipeline_indices_for_draws(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    swapchain_format: vk::Format,
    effect_target_plans: &[SceneEffectTargetImagePlan],
    scene_color_msaa_enabled: bool,
) -> Result<Vec<u32>, String> {
    let keys = drawn_pass_pipeline_keys(
        storage,
        graph,
        swapchain_format,
        effect_target_plans,
        scene_color_msaa_enabled,
    )?;
    let mut indices = vec![0u32; graph.mesh_draws.len()];
    for pass in graph
        .pass_nodes
        .iter()
        .filter(|pass| pass.mesh_draw_count != 0)
    {
        let pass_record = storage
            .document()
            .render_passes
            .get(pass.pass_record_index as usize)
            .ok_or_else(|| "scene drawable pass references a missing pass record".to_owned())?;
        let primitive = pass_draw_primitive(graph, pass)?;
        let key = ScenePipelineKey {
            shader: ScenePipelineShader::Authored(pass_record.shader_key),
            primitive,
            blend: scene_gpu_blend(storage, pass_record, pass.target),
            cull_mode: pass_record.cull_mode,
            color_write_mask: pass_record.color_write_mask,
            advanced_source_premultiplied: advanced_source_is_premultiplied(pass_record),
            advanced_blend_overlap: advanced_blend_overlap(storage, pass_record),
            target_format: pass_target_format(graph, pass, swapchain_format, effect_target_plans)?,
            samples: pass_pipeline_samples(pass.target, scene_color_msaa_enabled),
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

pub(in crate::renderer::native_vulkan) fn scene_disabled_pipeline_indices_for_draws(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    swapchain_format: vk::Format,
    effect_target_plans: &[SceneEffectTargetImagePlan],
    scene_color_msaa_enabled: bool,
) -> Result<Vec<Option<u32>>, String> {
    let keys = drawn_pass_pipeline_keys(
        storage,
        graph,
        swapchain_format,
        effect_target_plans,
        scene_color_msaa_enabled,
    )?;
    let mut indices = vec![None; graph.mesh_draws.len()];
    for pass in graph.pass_nodes.iter().filter(|pass| {
        pass.mesh_draw_count != 0
            && pass.effect_visibility_policy
                == crate::engine::scene::SceneRenderEffectVisibilityPolicy::Passthrough
    }) {
        let pass_record = storage
            .document()
            .render_passes
            .get(pass.pass_record_index as usize)
            .ok_or_else(|| "scene drawable pass references a missing pass record".to_owned())?;
        let primitive = pass_draw_primitive(graph, pass)?;
        let key = ScenePipelineKey {
            shader: ScenePipelineShader::EffectPassthrough(pass_record.shader_key),
            primitive,
            blend: SceneGpuBlend::Replace,
            cull_mode: pass_record.cull_mode,
            color_write_mask: pass_record.color_write_mask,
            advanced_source_premultiplied: false,
            advanced_blend_overlap: vk::BlendOverlapEXT::UNCORRELATED,
            target_format: pass_target_format(
                graph,
                pass,
                swapchain_format,
                effect_target_plans,
            )?,
            samples: pass_pipeline_samples(pass.target, scene_color_msaa_enabled),
        };
        let pipeline_index = keys
            .iter()
            .position(|candidate| *candidate == key)
            .ok_or_else(|| "scene disabled effect pass has no passthrough pipeline key".to_owned())?
            as u32;
        let start = pass.mesh_draw_start as usize;
        let end = start.saturating_add(pass.mesh_draw_count as usize);
        for slot in indices.get_mut(start..end).unwrap_or(&mut []) {
            *slot = Some(pipeline_index);
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
    scene_color_msaa_enabled: bool,
) -> Result<ScenePipelineResources, String> {
    let keys = drawn_pass_pipeline_keys(
        storage,
        graph,
        target_format,
        effect_target_plans,
        scene_color_msaa_enabled,
    )?;
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
        let (vertex_shader_key, fragment_shader_key) = match key.shader {
            ScenePipelineShader::Authored(shader_id) => {
                let shader_key = storage
                    .string(shader_id)
                    .ok_or_else(|| "scene drawable pass has no shader key".to_owned())?;
                (shader_key, shader_key)
            }
            ScenePipelineShader::EffectPassthrough(shader_id) => (
                storage
                    .string(shader_id)
                    .ok_or_else(|| "scene passthrough pass has no authored shader key".to_owned())?,
                "we/passthrough",
            ),
        };
        let vertex_shader = native_vulkan_scene_shader_for_key(vertex_shader_key).ok_or_else(|| {
            format!("scene vertex shader {vertex_shader_key:?} is not in the built-in catalog")
        })?;
        let fragment_shader = native_vulkan_scene_shader_for_key(fragment_shader_key)
            .ok_or_else(|| {
                format!(
                    "scene fragment shader {fragment_shader_key:?} is not in the built-in catalog"
            )
        })?;
        let descriptor_access = match key.shader {
            ScenePipelineShader::Authored(shader_id) => {
                scene_pipeline_shader_descriptor_access(storage, shader_id)?
            }
            ScenePipelineShader::EffectPassthrough(_) => scene_passthrough_descriptor_access(),
        };
        if !descriptor_access.input_attachment_slots.is_empty() {
            destroy_scene_pipelines(
                device,
                ScenePipelineResources {
                    entries,
                    particle_compute: None,
                },
            );
            return Err(format!(
                "scene shader {fragment_shader_key:?} declares input attachments but its subpassInput shader variant is not connected"
            ));
        }
        let vertex_spirv = native_vulkan_scene_vertex_spirv_for_primitive(
            vertex_shader,
            key.primitive,
        )
        .ok_or_else(|| {
            format!(
                "scene shader {vertex_shader_key:?} has no {:?} vertex program",
                key.primitive
            )
        })?;
        let pipeline_debug =
            std::env::var_os("GILDER_NATIVE_VULKAN_SCENE_PIPELINE_DEBUG").is_some();
        if pipeline_debug {
            eprintln!(
                "gilder-scene-pipeline-create: begin vertex={vertex_shader_key:?} fragment={fragment_shader_key:?} primitive={:?}",
                key.primitive
            );
        }
        match create_scene_pipeline(
            device,
            key.target_format,
            extent,
            vertex_spirv,
            fragment_shader.fragment_spirv,
            descriptor_heap_plan,
            descriptor_layout,
            &descriptor_access,
            key.blend,
            key.cull_mode,
            key.color_write_mask,
            key.advanced_source_premultiplied,
            key.advanced_blend_overlap,
            key.samples,
            if key.primitive == SceneRenderingDeviceDrawPrimitive::ParticleBillboard {
                vk::PrimitiveTopology::TRIANGLE_STRIP
            } else {
                vk::PrimitiveTopology::TRIANGLE_LIST
            },
        ) {
            Ok(pipeline) => {
                if pipeline_debug {
                    eprintln!(
                        "gilder-scene-pipeline-create: complete vertex={vertex_shader_key:?} fragment={fragment_shader_key:?} primitive={:?}",
                        key.primitive
                    );
                }
                entries.push(ScenePipelineEntry { key, pipeline });
            }
            Err(err) => {
                destroy_scene_pipelines(
                    device,
                    ScenePipelineResources {
                        entries,
                        particle_compute: None,
                    },
                );
                return Err(err);
            }
        }
    }
    let particle_compute = particle_compute::create_optional_particle_compute_pipeline(
        device,
        graph,
        descriptor_heap_plan,
    )?;
    Ok(ScenePipelineResources {
        entries,
        particle_compute,
    })
}

pub(in crate::renderer::native_vulkan) fn destroy_scene_pipelines(
    device: &Device,
    resources: ScenePipelineResources,
) {
    particle_compute::destroy_optional_particle_compute_pipeline(
        device,
        resources.particle_compute,
    );
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
    scene_color_msaa_enabled: bool,
) -> Result<Vec<ScenePipelineKey>, String> {
    let mut keys = Vec::<ScenePipelineKey>::new();
    for pass in graph
        .pass_nodes
        .iter()
        .filter(|pass| pass.mesh_draw_count != 0)
    {
        let pass_record = storage
            .document()
            .render_passes
            .get(pass.pass_record_index as usize)
            .ok_or_else(|| "scene drawable pass references a missing pass record".to_owned())?;
        if pass_record.shader_key == SceneStringId::NONE {
            return Err("scene drawable pass has no shader key".to_owned());
        }
        let primitive = pass_draw_primitive(graph, pass)?;
        validate_authored_shader_primitive(storage, pass_record.shader_key, primitive)?;
        let key = ScenePipelineKey {
            shader: ScenePipelineShader::Authored(pass_record.shader_key),
            primitive,
            blend: scene_gpu_blend(storage, pass_record, pass.target),
            cull_mode: pass_record.cull_mode,
            color_write_mask: pass_record.color_write_mask,
            advanced_source_premultiplied: advanced_source_is_premultiplied(pass_record),
            advanced_blend_overlap: advanced_blend_overlap(storage, pass_record),
            target_format: pass_target_format(graph, pass, swapchain_format, effect_target_plans)?,
            samples: pass_pipeline_samples(pass.target, scene_color_msaa_enabled),
        };
        if !keys.contains(&key) {
            keys.push(key);
        }
        if pass.effect_visibility_policy
            == crate::engine::scene::SceneRenderEffectVisibilityPolicy::Passthrough
        {
            let disabled = ScenePipelineKey {
                shader: ScenePipelineShader::EffectPassthrough(pass_record.shader_key),
                primitive,
                blend: SceneGpuBlend::Replace,
                cull_mode: pass_record.cull_mode,
                color_write_mask: pass_record.color_write_mask,
                advanced_source_premultiplied: false,
                advanced_blend_overlap: vk::BlendOverlapEXT::UNCORRELATED,
                target_format: key.target_format,
                samples: key.samples,
            };
            if !keys.contains(&disabled) {
                keys.push(disabled);
            }
        }
    }
    Ok(keys)
}

fn drawn_pass_material_keys(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
) -> Result<Vec<ScenePipelineKey>, String> {
    let mut keys = Vec::<ScenePipelineKey>::new();
    for pass in graph
        .pass_nodes
        .iter()
        .filter(|pass| pass.mesh_draw_count != 0)
    {
        let pass_record = storage
            .document()
            .render_passes
            .get(pass.pass_record_index as usize)
            .ok_or_else(|| "scene drawable pass references a missing pass record".to_owned())?;
        if pass_record.shader_key == SceneStringId::NONE {
            return Err("scene drawable pass has no shader key".to_owned());
        }
        let primitive = pass_draw_primitive(graph, pass)?;
        validate_authored_shader_primitive(storage, pass_record.shader_key, primitive)?;
        let key = ScenePipelineKey {
            shader: ScenePipelineShader::Authored(pass_record.shader_key),
            primitive,
            blend: scene_gpu_blend(storage, pass_record, pass.target),
            cull_mode: pass_record.cull_mode,
            color_write_mask: pass_record.color_write_mask,
            advanced_source_premultiplied: advanced_source_is_premultiplied(pass_record),
            advanced_blend_overlap: advanced_blend_overlap(storage, pass_record),
            target_format: vk::Format::UNDEFINED,
            samples: ScenePipelineSamples::Single,
        };
        if !keys.contains(&key) {
            keys.push(key);
        }
    }
    Ok(keys)
}

fn pass_draw_primitive(
    graph: &SceneRenderingDeviceGraphPlan,
    pass: &SceneRenderingDevicePassNode,
) -> Result<SceneRenderingDeviceDrawPrimitive, String> {
    let start = pass.mesh_draw_start as usize;
    let end = start
        .checked_add(pass.mesh_draw_count as usize)
        .ok_or_else(|| "scene drawable pass draw range overflows".to_owned())?;
    let draws = graph.mesh_draws.get(start..end).ok_or_else(|| {
        format!(
            "scene drawable pass {} draw range {}..{} is outside {} draws",
            pass.pass_id,
            start,
            end,
            graph.mesh_draws.len()
        )
    })?;
    let primitive = draws
        .first()
        .ok_or_else(|| format!("scene drawable pass {} has an empty draw range", pass.pass_id))?
        .primitive;
    if draws.iter().any(|draw| draw.primitive != primitive) {
        return Err(format!(
            "scene drawable pass {} mixes incompatible draw primitives",
            pass.pass_id
        ));
    }
    Ok(primitive)
}

fn validate_authored_shader_primitive(
    storage: &SceneStorage,
    shader_id: SceneStringId,
    primitive: SceneRenderingDeviceDrawPrimitive,
) -> Result<(), String> {
    let shader_key = storage
        .string(shader_id)
        .ok_or_else(|| "scene drawable pass has no shader key".to_owned())?;
    let shader = native_vulkan_scene_shader_for_key(shader_key)
        .ok_or_else(|| format!("scene shader {shader_key:?} is not in the built-in catalog"))?;
    native_vulkan_scene_vertex_spirv_for_primitive(shader, primitive)
        .map(|_| ())
        .ok_or_else(|| {
            format!(
                "scene shader {shader_key:?} has no {primitive:?} vertex program"
            )
        })
}

fn scene_gpu_blend(
    storage: &SceneStorage,
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
        SceneCompositeBlend::Alpha => SceneGpuBlend::Alpha,
        SceneCompositeBlend::Normal => pipeline_gpu_blend(pass.pipeline_blend),
        SceneCompositeBlend::Additive => SceneGpuBlend::Additive,
        SceneCompositeBlend::Multiply => {
            if storage
                .string(pass.shader_key)
                .is_some_and(|shader| shader == "we/image-effect-composite__STATIC_BLACK_1")
            {
                SceneGpuBlend::Alpha
            } else if storage
                .string(pass.shader_key)
                .is_some_and(multiply_shader_is_premultiplied)
            {
                SceneGpuBlend::MultiplyPremultiplied
            } else {
                SceneGpuBlend::Multiply
            }
        }
        SceneCompositeBlend::Screen => {
            if storage.string(pass.shader_key).is_some_and(|shader| {
                shader
                    .to_ascii_lowercase()
                    .strip_prefix("we/image-foliage-ripple-screen-composite")
                    .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with("__"))
            }) {
                SceneGpuBlend::ScreenPremultiplied
            } else {
                SceneGpuBlend::Screen
            }
        }
        SceneCompositeBlend::Max => SceneGpuBlend::Maximum,
        SceneCompositeBlend::Modulate => SceneGpuBlend::Modulate,
        SceneCompositeBlend::HslColor => SceneGpuBlend::HslColor,
        SceneCompositeBlend::AlphaToCoverage => SceneGpuBlend::AlphaToCoverage,
    }
}

fn multiply_shader_is_premultiplied(shader: &str) -> bool {
    let shader = shader.to_ascii_lowercase();
    matches!(
        shader.as_str(),
        "we/genericimage4-multiply-composite"
            | "we/image-waterwaves-multiply-composite"
            | "we/image-ripple-flow-multiply-composite"
    ) || shader
        .strip_prefix("we/image-waterwaves-multiply-direct")
        .is_some_and(|suffix| suffix.is_empty() || suffix.starts_with("__stages_"))
}

fn advanced_source_is_premultiplied(pass: &SceneRenderPassRecord) -> bool {
    pass.role == SceneRenderPassKind::EffectMaterial
        && matches!(
            pass.scene_blend,
            SceneCompositeBlend::Multiply
                | SceneCompositeBlend::Screen
                | SceneCompositeBlend::HslColor
        )
}

fn advanced_blend_overlap(
    storage: &SceneStorage,
    pass: &SceneRenderPassRecord,
) -> vk::BlendOverlapEXT {
    if pass.scene_blend == SceneCompositeBlend::HslColor
        && storage
            .string(pass.shader_key)
            .is_some_and(|shader| shader.eq_ignore_ascii_case("we/flat-rounded-mask-composite"))
    {
        vk::BlendOverlapEXT::DISJOINT
    } else {
        vk::BlendOverlapEXT::UNCORRELATED
    }
}

fn pipeline_gpu_blend(blend: ScenePipelineBlend) -> SceneGpuBlend {
    match blend {
        ScenePipelineBlend::Normal | ScenePipelineBlend::Disabled => SceneGpuBlend::Replace,
        ScenePipelineBlend::Translucent => SceneGpuBlend::Alpha,
        ScenePipelineBlend::Additive => SceneGpuBlend::Additive,
        ScenePipelineBlend::AlphaToCoverage => SceneGpuBlend::AlphaToCoverage,
    }
}

fn pass_pipeline_samples(
    target: SceneRenderTargetKind,
    scene_color_msaa_enabled: bool,
) -> ScenePipelineSamples {
    if scene_color_msaa_enabled
        && matches!(
            target,
            SceneRenderTargetKind::SceneColor | SceneRenderTargetKind::Swapchain
        )
    {
        ScenePipelineSamples::SceneColor4x
    } else {
        ScenePipelineSamples::Single
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
    descriptor_access: &ScenePipelineShaderDescriptorAccess,
    blend: SceneGpuBlend,
    cull_mode: SceneCullMode,
    color_write_mask: SceneColorWriteMask,
    advanced_source_premultiplied: bool,
    advanced_blend_overlap: vk::BlendOverlapEXT,
    samples: ScenePipelineSamples,
    topology: vk::PrimitiveTopology,
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
            descriptor_access,
            blend,
            cull_mode,
            color_write_mask,
            advanced_source_premultiplied,
            advanced_blend_overlap,
            samples,
            topology,
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
    descriptor_access: &ScenePipelineShaderDescriptorAccess,
    blend: SceneGpuBlend,
    cull_mode: SceneCullMode,
    color_write_mask: SceneColorWriteMask,
    advanced_source_premultiplied: bool,
    advanced_blend_overlap: vk::BlendOverlapEXT,
    samples: ScenePipelineSamples,
    topology: vk::PrimitiveTopology,
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
    if descriptor_layout.material_uniform_enabled {
        vertex_mappings.push(
            native_vulkan_vulkanalia_descriptor_heap_resource_relative_uniform_buffer_binding_mapping(
                descriptor_heap_plan,
                3,
                0,
                1,
            )?,
        );
    }
    let skinning_descriptor_index = 1 + usize::from(descriptor_layout.material_uniform_enabled);
    if descriptor_layout.skinning_storage_enabled {
        vertex_mappings.push(
            native_vulkan_vulkanalia_descriptor_heap_resource_relative_storage_buffer_binding_mapping(
                descriptor_heap_plan,
                4,
                0,
                skinning_descriptor_index,
                false,
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
    for slot in &descriptor_access.sampled_slots {
        let sampled_index = descriptor_layout
            .sampled_slots
            .iter()
            .position(|candidate| candidate == slot)
            .ok_or_else(|| {
                format!("scene shader sampled slot {slot} is absent from the global descriptor layout")
            })?;
        fragment_mappings.push(
            native_vulkan_vulkanalia_descriptor_heap_resource_relative_combined_image_sampler_binding_mapping(
                descriptor_heap_plan,
                scene_sampled_shader_binding(*slot),
                0,
                descriptor_layout.sampled_resource_offset() + sampled_index,
                0,
                sampled_index,
            )?,
        );
    }
    for slot in &descriptor_access.input_attachment_slots {
        let input_index = descriptor_layout
            .input_attachment_slots
            .iter()
            .position(|candidate| candidate == slot)
            .ok_or_else(|| {
                format!("scene shader input-attachment slot {slot} is absent from the global descriptor layout")
            })?;
        fragment_mappings.push(
            native_vulkan_vulkanalia_descriptor_heap_resource_relative_mixed_input_attachment_binding_mapping(
                descriptor_heap_plan,
                scene_input_attachment_shader_binding(*slot),
                0,
                descriptor_layout.input_attachment_resource_offset() + input_index,
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
        cull_mode,
        color_write_mask,
        advanced_source_premultiplied,
        advanced_blend_overlap,
        samples,
        topology,
    )
}

fn scene_sampled_shader_binding(slot: u32) -> u32 {
    // Fragment binding 3 is reserved for material/effect uniforms. WE's
    // logical texture slot stays 3 in IR and heap planning, while SPIR-V uses
    // a collision-free binding selected here and in build.rs.
    if slot == 3 { 35 } else { slot }
}

fn scene_input_attachment_shader_binding(slot: u32) -> u32 {
    // Input attachments use the same logical slot namespace as sampled images,
    // but a separate SPIR-V binding namespace until the subpassInput catalog
    // variants are connected.  The mapping remains sampler-free.
    64 + scene_sampled_shader_binding(slot)
}

fn create_graphics_pipeline(
    device: &Device,
    target_format: vk::Format,
    stages: [vk::PipelineShaderStageCreateInfo; 2],
    blend: SceneGpuBlend,
    cull_mode: SceneCullMode,
    color_write_mask: SceneColorWriteMask,
    advanced_source_premultiplied: bool,
    advanced_blend_overlap: vk::BlendOverlapEXT,
    samples: ScenePipelineSamples,
    topology: vk::PrimitiveTopology,
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
        .topology(topology)
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
        .cull_mode(scene_vk_cull_mode(cull_mode))
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0)
        .build();
    let multisample = vk::PipelineMultisampleStateCreateInfo::builder()
        .rasterization_samples(samples.rasterization_samples())
        .alpha_to_coverage_enable(blend == SceneGpuBlend::AlphaToCoverage)
        .build();
    let color_attachment = scene_color_blend_attachment(blend, color_write_mask);
    let color_attachments = [color_attachment];
    let mut advanced_blend = vk::PipelineColorBlendAdvancedStateCreateInfoEXT::builder()
        .src_premultiplied(advanced_source_premultiplied)
        .dst_premultiplied(false)
        .blend_overlap(advanced_blend_overlap)
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

fn scene_vk_cull_mode(cull_mode: SceneCullMode) -> vk::CullModeFlags {
    match cull_mode {
        SceneCullMode::None => vk::CullModeFlags::NONE,
        SceneCullMode::Normal => vk::CullModeFlags::BACK,
    }
}

#[cfg(test)]
mod tests;
