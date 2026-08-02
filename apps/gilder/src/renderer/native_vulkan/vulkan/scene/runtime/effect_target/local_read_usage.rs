//! Image-usage planning for retained dynamic-rendering local-read scopes.

use super::super::input_attachment_binding::{
    SceneInputAttachmentBindingPlan, SceneInputAttachmentSource,
};
use super::super::local_read::SceneLocalReadScopePlan;
use super::super::sampled_binding::{
    SceneSampledImageBindingPlan, logical_target_references as sampled_logical_target_references,
    reference_physical_slot,
};
use super::*;

pub(in crate::renderer::native_vulkan) fn apply_scene_effect_target_input_attachment_usage(
    plans: &mut [SceneEffectTargetImagePlan],
    binding_cycle: &[SceneInputAttachmentBindingPlan],
) -> Result<(), String> {
    for source in binding_cycle
        .iter()
        .flat_map(|phase| phase.sources.iter().flatten())
    {
        let SceneInputAttachmentSource::EffectTarget {
            physical_slot,
            batch_atlas_tile,
        } = *source;
        if batch_atlas_tile != 0 {
            return Err(format!(
                "scene input-attachment physical slot {physical_slot} uses unsupported atlas tile {batch_atlas_tile}"
            ));
        }
        let plan = plans
            .iter_mut()
            .find(|plan| plan.physical_slot == physical_slot)
            .ok_or_else(|| {
                format!(
                    "scene input-attachment physical slot {physical_slot} has no effect-target image plan"
                )
            })?;
        plan.input_attachment_required = true;
    }
    Ok(())
}

/// Marks the initial image plans for every structurally possible local-read
/// attachment before scope planning validates the full contract.  The
/// destination is not itself an input-attachment source, so waiting until
/// after scope planning would reject a valid final consumer before its
/// required `INPUT_ATTACHMENT_BIT` can be added.
pub(in crate::renderer::native_vulkan) fn apply_scene_effect_target_local_read_candidate_usage(
    plans: &mut [SceneEffectTargetImagePlan],
    graph: &SceneRenderingDeviceGraphPlan,
    sampled_cycle: &[SceneSampledImageBindingPlan],
) -> Result<(), String> {
    for (consumer_index, consumer) in graph.pass_nodes.iter().enumerate() {
        let Some(binding) = graph.sampled_bindings.iter().find(|binding| {
            binding.pass_node_index == consumer_index as u32
                && binding.access == SceneRenderingDeviceImageAccess::InputAttachment
        }) else {
            continue;
        };
        let Some(producer) = consumer_index
            .checked_sub(1)
            .and_then(|index| graph.pass_nodes.get(index))
        else {
            continue;
        };
        let Some((source_graph, source_target, source_name)) = binding.logical_target() else {
            continue;
        };
        for (graph_index, target, target_name) in [
            (source_graph, source_target, source_name),
            (consumer.graph_index, consumer.target, consumer.target_name),
            (producer.graph_index, producer.target, producer.target_name),
        ] {
            if graph.target_allocations.iter().all(|allocation| {
                (
                    allocation.graph_index,
                    allocation.target,
                    allocation.target_name,
                ) != (graph_index, target, target_name)
            }) {
                continue;
            }
            mark_logical_target_usage_for_cycle(
                plans,
                graph,
                sampled_cycle,
                graph_index,
                target,
                target_name,
            )?;
        }
    }
    Ok(())
}

/// A local-read scope keeps both of its color attachments in
/// `VK_IMAGE_LAYOUT_RENDERING_LOCAL_READ` for the duration of the merged
/// rendering instance. Vulkan therefore requires the destination attachment
/// to carry input-attachment usage as well as the source, even though only the
/// source is read by the consumer shader. Mark every physical slot that the
/// two logical scope targets occupy across the retained reference cycle; an
/// initial-slot-only mark would become invalid after an authored swap.
pub(in crate::renderer::native_vulkan) fn apply_scene_effect_target_local_read_scope_usage(
    plans: &mut [SceneEffectTargetImagePlan],
    graph: &SceneRenderingDeviceGraphPlan,
    scopes: &[SceneLocalReadScopePlan],
    sampled_cycle: &[SceneSampledImageBindingPlan],
) -> Result<(), String> {
    for scope in scopes {
        for target in [scope.source(), scope.destination()] {
            mark_logical_target_usage_for_cycle(
                plans,
                graph,
                sampled_cycle,
                target.graph_index(),
                target.target(),
                target.target_name(),
            )?;
        }
    }
    Ok(())
}

fn mark_logical_target_usage_for_cycle(
    plans: &mut [SceneEffectTargetImagePlan],
    graph: &SceneRenderingDeviceGraphPlan,
    sampled_cycle: &[SceneSampledImageBindingPlan],
    graph_index: u32,
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
) -> Result<(), String> {
    for phase in sampled_cycle {
        let mut references = sampled_logical_target_references(&graph.target_allocations);
        if references.len() != phase.initial_reference_physical_slots.len() {
            return Err(format!(
                "scene local-read target {:?}:{:?} has {} target references but sampled phase has {}",
                target,
                target_name,
                references.len(),
                phase.initial_reference_physical_slots.len()
            ));
        }
        for (reference, physical_slot) in references
            .iter_mut()
            .zip(phase.initial_reference_physical_slots.iter().copied())
        {
            reference.physical_slot = physical_slot;
        }
        let physical_slot = reference_physical_slot(&references, graph_index, target, target_name)
            .ok_or_else(|| {
                format!(
                    "scene local-read target {:?}:{:?} has no physical slot in sampled phase",
                    target, target_name
                )
            })?;
        let plan = plans
            .iter_mut()
            .find(|plan| plan.physical_slot == physical_slot)
            .ok_or_else(|| {
                format!(
                    "scene local-read physical slot {physical_slot} has no effect-target image plan"
                )
            })?;
        plan.input_attachment_required = true;
    }
    Ok(())
}
