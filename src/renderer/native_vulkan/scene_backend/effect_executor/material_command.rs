//! Effect material-pass runtime recording.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/effect-semantics.md`
//! - `reverse-engineered/effects/iris.md`
//! - `references/godot/servers/rendering/renderer_rd/effects/tone_mapper.cpp`
//! - `references/godot/servers/rendering/rendering_device_graph.h`

use std::collections::BTreeSet;

use vulkanalia::vk;

use crate::engine::scene_engine::{
    SceneEffectPassGraphInputSource, SceneEffectPassGraphMaterialPass, SceneGraphTarget,
};

use super::super::effect_pass_command::{
    NativeVulkanSceneEffectPassCommandPlan,
    native_vulkan_record_scene_effect_material_pass_commands,
};
use super::super::effect_pipeline::NativeVulkanSceneEffectPipelineCacheKey;
use super::super::frame_resources::NativeVulkanSceneFrameResources;
use super::super::render_target::{
    NativeVulkanSceneRenderTargetScopePlan, native_vulkan_record_scene_render_target_begin,
    native_vulkan_record_scene_render_target_end,
};
use super::NativeVulkanSceneEffectRuntimeFrameContext;
use super::target_access::{
    NativeVulkanSceneEffectTargetAccessPlan, NativeVulkanSceneEffectTargetTransitionPlan,
    effect_offscreen_render_target, effect_pass_render_target, effect_target_format,
    record_effect_target_shader_read_access, record_effect_target_transition,
    transparent_clear_color,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneEffectMaterialRuntimeCommandPlan<'a>
{
    pub graph_command_index: usize,
    pub output: SceneGraphTarget,
    pub input_access_count: usize,
    pub output_transition_count: usize,
    pub target_scope: NativeVulkanSceneRenderTargetScopePlan,
    pub input_accesses: Vec<NativeVulkanSceneEffectTargetAccessPlan>,
    pub output_transition: Option<NativeVulkanSceneEffectTargetTransitionPlan>,
    pub pass: NativeVulkanSceneEffectPassCommandPlan<'a>,
    pub command_order: [&'static str; 5],
}

pub(super) fn record_effect_material_pass<'a>(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    context: &NativeVulkanSceneEffectRuntimeFrameContext<'_>,
    pass: &'a SceneEffectPassGraphMaterialPass,
    written_targets: &mut BTreeSet<SceneGraphTarget>,
) -> Result<NativeVulkanSceneEffectMaterialRuntimeCommandPlan<'a>, String> {
    let output = effect_pass_render_target(pass)?;
    let input_accesses =
        record_effect_material_input_accesses(frame_resources, context, pass, output)?;
    let first_write = written_targets.insert(output);
    let output_transition = record_effect_target_transition(
        frame_resources,
        context.device,
        context.command_buffer,
        output,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        "effect-material-output-color-write",
    )?;
    let render_target = effect_offscreen_render_target(frame_resources, output)?;
    let clear_color = first_write.then_some(transparent_clear_color());
    let target_scope = native_vulkan_record_scene_render_target_begin(
        context.device,
        context.command_buffer,
        render_target,
        clear_color,
    )?;
    let pass_target_format = effect_target_format(frame_resources, context.target_formats, output)?;
    let bind_info = frame_resources.effect_resource_heap_pass_bind_info(pass.graph_pass_index)?;
    let pass_plan = {
        let resources = &*frame_resources;
        native_vulkan_record_scene_effect_material_pass_commands(
            context.device,
            context.command_buffer,
            pass,
            pass_target_format,
            bind_info,
            |key| {
                let cache_key = NativeVulkanSceneEffectPipelineCacheKey::from_bind_key(key);
                Ok(resources.cached_effect_pipeline(&cache_key)?.pipeline)
            },
        )?
    };
    native_vulkan_record_scene_render_target_end(
        context.device,
        context.command_buffer,
        render_target,
        clear_color,
    )?;
    frame_resources
        .mark_offscreen_target_layout(output, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)?;

    Ok(NativeVulkanSceneEffectMaterialRuntimeCommandPlan {
        graph_command_index: pass.graph_command_index,
        output,
        input_access_count: input_accesses.len(),
        output_transition_count: output_transition.iter().count(),
        target_scope,
        input_accesses,
        output_transition,
        pass: pass_plan,
        command_order: [
            "transition_effect_graph_inputs_to_shader_read",
            "transition_effect_output_to_color_attachment",
            "cmd_begin_rendering_effect_target",
            "record_effect_material_pass_commands",
            "cmd_end_rendering_effect_target",
        ],
    })
}

fn record_effect_material_input_accesses(
    frame_resources: &mut NativeVulkanSceneFrameResources,
    context: &NativeVulkanSceneEffectRuntimeFrameContext<'_>,
    pass: &SceneEffectPassGraphMaterialPass,
    output: SceneGraphTarget,
) -> Result<Vec<NativeVulkanSceneEffectTargetAccessPlan>, String> {
    let mut accesses = Vec::new();
    let mut seen = BTreeSet::new();
    for source in pass
        .source
        .iter()
        .chain(pass.input_bindings.iter())
        .map(|binding| &binding.source)
    {
        let SceneEffectPassGraphInputSource::GraphTarget(target) = source else {
            continue;
        };
        if *target == output {
            return Err(format!(
                "scene effect pass {} for object {:?} reads and writes {:?} in the same material pass; explicit ping-pong FBOs are required",
                pass.pass_index, pass.object, target
            ));
        }
        if !seen.insert(*target) {
            continue;
        }
        if let Some(access) = record_effect_target_shader_read_access(
            frame_resources,
            context.device,
            context.command_buffer,
            *target,
        )? {
            accesses.push(access);
        }
    }
    Ok(accesses)
}
