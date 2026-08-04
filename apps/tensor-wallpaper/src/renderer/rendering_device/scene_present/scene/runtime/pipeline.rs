//! Scene Vulkan pipeline selection for mesh and fullscreen effect draws.
//!
//! References:
//! - `docs/tensor-wallpaper/tensor-wallpaper-scene-engine-architecture.md`
//! - `reverse-engineered/tensor-wallpaper/docs/effect-format.md`
//! - `references/tensor-wallpaper/godot/servers/rendering/renderer_rd/pipeline_hash_map_rd.h`
//! - `references/tensor-wallpaper/godot/drivers/vulkan/rendering_device_driver_vulkan.*`

use vulkan_renderer::{BlendOverlap, TextureFormat};

use crate::engine::scene::{
    SceneColorWriteMask, SceneCompositeBlend, SceneCullMode, ScenePipelineBlend,
    SceneRenderPassKind, SceneRenderPassRecord, SceneRenderTargetKind,
    SceneRenderingDeviceDrawPrimitive, SceneRenderingDeviceGraphPlan, SceneRenderingDevicePassNode,
    SceneStorage, SceneStringId,
};
use crate::renderer::rendering_device::scene::{
    BuiltinSceneLocalReadShader, rendering_device_scene_shader_for_key,
    rendering_device_scene_vertex_shader_for_primitive,
};

use super::descriptor_layout::{
    ScenePipelineShaderDescriptorAccess, scene_passthrough_descriptor_access,
    scene_pipeline_shader_descriptor_access,
};
use super::effect_target::SceneEffectTargetImagePlan;
use super::local_read::{
    SceneLocalReadPipelineMetadata, SceneLocalReadScopePassRole, SceneLocalReadScopePlan,
};
use super::shader_program::{
    SceneResolvedGraphicsProgram, SceneVertexAttributePlan, resolve_scene_graphics_program,
    scene_owned_vertex_attributes,
};

mod creation;
mod diagnostics;
mod graphics;
mod local_read_key;
mod particle_compute;
mod samples;
mod video;

use graphics::create_graphics_pipeline;
use local_read_key::{ScenePipelineLocalReadRole, local_read_pipeline_role};
use samples::ScenePipelineSamples;

pub(super) fn create_scene_terminal_present_pipeline(
    device: &vulkan_renderer::Backend,
    target_format: TextureFormat,
    extent: vulkan_renderer::Extent2D,
    vertex_spirv: &[u32],
    fragment_spirv: &[u32],
    pipeline_binary_cache: &vulkan_renderer::PipelineBinaryArchiveCache,
) -> Result<vulkan_renderer::MachineCodeGraphicsPipeline, String> {
    creation::create_scene_pipeline(creation::SceneGraphicsPipelineCreateInputs {
        device,
        target_format,
        extent,
        vertex_spirv,
        fragment_spirv,
        vertex_entry_point: "main",
        fragment_entry_point: "main",
        vertex_attributes: Some(&[]),
        local_read_metadata: None,
        blend: SceneGpuBlend::Replace,
        cull_mode: SceneCullMode::None,
        color_write_mask: SceneColorWriteMask::Rgba,
        advanced_source_premultiplied: false,
        advanced_blend_overlap: BlendOverlap::Uncorrelated,
        samples: ScenePipelineSamples::Single,
        topology: vulkan_renderer::PrimitiveTopology::TriangleList,
        default_mesh_vertex_input: false,
        dynamic_text: false,
        pipeline_binary_cache,
    })
}

pub(in crate::renderer::rendering_device) struct ScenePipelineResources {
    pub entries: Vec<ScenePipelineEntry>,
    pub video: Option<video::SceneVideoPipeline>,
    pub particle_compute: Option<particle_compute::SceneParticleComputePipeline>,
    pub machine_code_binary_count: usize,
    pub machine_code_bytes: usize,
    pub machine_code_cache_hits: usize,
    pub machine_code_cache_misses: usize,
}

pub(in crate::renderer::rendering_device) struct ScenePipelineEntry {
    key: ScenePipelineKey,
    pub pipeline: vulkan_renderer::MachineCodeGraphicsPipeline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScenePipelineKey {
    shader: ScenePipelineShader,
    primitive: SceneRenderingDeviceDrawPrimitive,
    blend: SceneGpuBlend,
    cull_mode: SceneCullMode,
    color_write_mask: SceneColorWriteMask,
    advanced_source_premultiplied: bool,
    advanced_blend_overlap: BlendOverlap,
    target_format: Option<TextureFormat>,
    samples: ScenePipelineSamples,
    local_read_role: Option<ScenePipelineLocalReadRole>,
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

pub(super) fn scene_requires_coherent_advanced_blend(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
) -> Result<bool, String> {
    graph
        .pass_nodes
        .iter()
        .filter(|pass| pass.mesh_draw_count != 0)
        .try_fold(false, |required, pass| {
            let record = storage
                .document()
                .render_passes
                .get(pass.pass_record_index as usize)
                .ok_or_else(|| "scene drawable pass references a missing pass record".to_owned())?;
            Ok(required
                || scene_gpu_blend(storage, record, pass.target).requires_advanced_operation())
        })
}

pub(in crate::renderer::rendering_device) fn scene_pipeline_indices_for_draws(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    swapchain_format: TextureFormat,
    effect_target_plans: &[SceneEffectTargetImagePlan],
    scene_color_msaa_enabled: bool,
) -> Result<Vec<u32>, String> {
    scene_pipeline_indices_for_draws_with_local_read(
        storage,
        graph,
        swapchain_format,
        effect_target_plans,
        &[],
        scene_color_msaa_enabled,
    )
}

pub(in crate::renderer::rendering_device) fn scene_pipeline_indices_for_draws_with_local_read(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    swapchain_format: TextureFormat,
    effect_target_plans: &[SceneEffectTargetImagePlan],
    local_read_scopes: &[SceneLocalReadScopePlan],
    scene_color_msaa_enabled: bool,
) -> Result<Vec<u32>, String> {
    let keys = drawn_pass_pipeline_keys(
        storage,
        graph,
        swapchain_format,
        effect_target_plans,
        local_read_scopes,
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
            target_format: Some(pass_target_format(
                graph,
                pass,
                swapchain_format,
                effect_target_plans,
            )?),
            samples: pass_pipeline_samples(pass.target, scene_color_msaa_enabled),
            local_read_role: local_read_pipeline_role(local_read_scopes, pass, false)?,
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

pub(in crate::renderer::rendering_device) fn scene_disabled_pipeline_indices_for_draws(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    swapchain_format: TextureFormat,
    effect_target_plans: &[SceneEffectTargetImagePlan],
    scene_color_msaa_enabled: bool,
) -> Result<Vec<Option<u32>>, String> {
    scene_disabled_pipeline_indices_for_draws_with_local_read(
        storage,
        graph,
        swapchain_format,
        effect_target_plans,
        &[],
        scene_color_msaa_enabled,
    )
}

pub(in crate::renderer::rendering_device) fn scene_disabled_pipeline_indices_for_draws_with_local_read(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    swapchain_format: TextureFormat,
    effect_target_plans: &[SceneEffectTargetImagePlan],
    local_read_scopes: &[SceneLocalReadScopePlan],
    scene_color_msaa_enabled: bool,
) -> Result<Vec<Option<u32>>, String> {
    let keys = drawn_pass_pipeline_keys(
        storage,
        graph,
        swapchain_format,
        effect_target_plans,
        local_read_scopes,
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
            advanced_blend_overlap: BlendOverlap::Uncorrelated,
            target_format: Some(pass_target_format(
                graph,
                pass,
                swapchain_format,
                effect_target_plans,
            )?),
            samples: pass_pipeline_samples(pass.target, scene_color_msaa_enabled),
            local_read_role: local_read_pipeline_role(local_read_scopes, pass, true)?,
        };
        let pipeline_index = keys
            .iter()
            .position(|candidate| *candidate == key)
            .ok_or_else(|| {
                "scene disabled effect pass has no passthrough pipeline key".to_owned()
            })? as u32;
        let start = pass.mesh_draw_start as usize;
        let end = start.saturating_add(pass.mesh_draw_count as usize);
        for slot in indices.get_mut(start..end).unwrap_or(&mut []) {
            *slot = Some(pipeline_index);
        }
    }
    Ok(indices)
}

pub(in crate::renderer::rendering_device) use creation::{
    ScenePipelineResourceCreateInputs, create_scene_pipelines,
};

fn drawn_pass_pipeline_keys(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    swapchain_format: TextureFormat,
    effect_target_plans: &[SceneEffectTargetImagePlan],
    local_read_scopes: &[SceneLocalReadScopePlan],
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
            target_format: Some(pass_target_format(
                graph,
                pass,
                swapchain_format,
                effect_target_plans,
            )?),
            samples: pass_pipeline_samples(pass.target, scene_color_msaa_enabled),
            local_read_role: local_read_pipeline_role(local_read_scopes, pass, false)?,
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
                advanced_blend_overlap: BlendOverlap::Uncorrelated,
                target_format: key.target_format,
                samples: key.samples,
                local_read_role: local_read_pipeline_role(local_read_scopes, pass, true)?,
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
            target_format: None,
            samples: ScenePipelineSamples::Single,
            local_read_role: None,
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
        .ok_or_else(|| {
            format!(
                "scene drawable pass {} has an empty draw range",
                pass.pass_id
            )
        })?
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
    resolve_scene_graphics_program(storage, shader_id, primitive).map(|_| ())
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

fn advanced_blend_overlap(storage: &SceneStorage, pass: &SceneRenderPassRecord) -> BlendOverlap {
    if pass.scene_blend == SceneCompositeBlend::HslColor
        && storage
            .string(pass.shader_key)
            .is_some_and(|shader| shader.eq_ignore_ascii_case("we/flat-rounded-mask-composite"))
    {
        BlendOverlap::Disjoint
    } else {
        BlendOverlap::Uncorrelated
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
    swapchain_format: TextureFormat,
    effect_target_plans: &[SceneEffectTargetImagePlan],
) -> Result<TextureFormat, String> {
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

#[cfg(test)]
mod tests;
