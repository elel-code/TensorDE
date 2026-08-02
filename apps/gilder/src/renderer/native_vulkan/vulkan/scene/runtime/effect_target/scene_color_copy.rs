//! Backend-neutral coverage planning for scene-color snapshot copies.

use crate::engine::scene::{
    SceneRenderingDeviceGraphPlan, SceneRenderingDeviceImageAccess, SceneStorage,
};
use crate::renderer::native_vulkan::scene::native_vulkan_scene_shader_for_key;

use super::{
    LogicalEffectTargetKey, SceneEffectTargetCommand, SceneEffectTargetCommandKind,
    SceneEffectTargetCommandSource,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan::vulkan::scene::runtime) enum SceneColorCopyCoverage {
    #[default]
    FullTarget,
    ConsumerDrawScissors {
        draw_start: u32,
        draw_count: u32,
    },
}

pub(super) fn scene_color_copy_coverage(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    copy_pass_node_index: usize,
    target: LogicalEffectTargetKey,
) -> SceneColorCopyCoverage {
    let target_key = (target.graph_index, target.target, target.name);
    let mut consumers = graph
        .sampled_bindings
        .iter()
        .filter(|binding| binding.logical_target() == Some(target_key));
    let Some(binding) = consumers.next() else {
        return SceneColorCopyCoverage::FullTarget;
    };
    if consumers.next().is_some()
        || binding.access != SceneRenderingDeviceImageAccess::SampledImage
        || binding.mesh_draw_count == 0
    {
        return SceneColorCopyCoverage::FullTarget;
    }
    let consumer_index = binding.pass_node_index as usize;
    let Some(consumer) = graph.pass_nodes.get(consumer_index) else {
        return SceneColorCopyCoverage::FullTarget;
    };
    if consumer_index <= copy_pass_node_index
        || consumer.graph_index != target.graph_index
        || (consumer.mesh_draw_start, consumer.mesh_draw_count)
            != (binding.mesh_draw_start, binding.mesh_draw_count)
        || graph
            .pass_nodes
            .get(copy_pass_node_index + 1..consumer_index)
            .is_none_or(|passes| {
                passes
                    .iter()
                    .any(|pass| LogicalEffectTargetKey::from_pass_target(pass) == Some(target))
            })
    {
        return SceneColorCopyCoverage::FullTarget;
    }
    let Some(record) = storage
        .document()
        .render_passes
        .get(consumer.pass_record_index as usize)
    else {
        return SceneColorCopyCoverage::FullTarget;
    };
    let Some(shader_key) = storage.string(record.shader_key) else {
        return SceneColorCopyCoverage::FullTarget;
    };
    let Some(shader) = native_vulkan_scene_shader_for_key(shader_key) else {
        return SceneColorCopyCoverage::FullTarget;
    };
    let Some(slot_bit) = 1u32.checked_shl(binding.slot) else {
        return SceneColorCopyCoverage::FullTarget;
    };
    if shader.fragment_coordinate_fetch_slot_mask & slot_bit == 0 {
        return SceneColorCopyCoverage::FullTarget;
    }
    SceneColorCopyCoverage::ConsumerDrawScissors {
        draw_start: binding.mesh_draw_start,
        draw_count: binding.mesh_draw_count,
    }
}

pub(in crate::renderer::native_vulkan) fn graph_copies_scene_color(
    commands: &[SceneEffectTargetCommand],
    graph_index: u32,
) -> bool {
    commands.iter().any(|command| {
        command.target.graph_index == graph_index
            && command.kind == SceneEffectTargetCommandKind::Copy
            && command.source == Some(SceneEffectTargetCommandSource::SceneColor)
    })
}

pub(in crate::renderer::native_vulkan) fn graph_requires_effect_target_execution(
    commands: &[SceneEffectTargetCommand],
    graph_index: u32,
) -> bool {
    commands.iter().any(|command| {
        command.target.graph_index == graph_index && command.batch_atlas_tile.is_none()
    })
}

pub(in crate::renderer::native_vulkan) fn graph_uses_direct_scene_color_snapshot(
    commands: &[SceneEffectTargetCommand],
    graph_index: u32,
) -> bool {
    commands.iter().any(|command| {
        command.target.graph_index == graph_index && command.direct_scene_color_snapshot
    })
}
