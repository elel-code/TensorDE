//! Per-draw sampled-image lowering for scene descriptor heaps.
//!
//! References:
//! - `docs/gilder/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/gilder/docs/effect-format.md`
//! - `references/gilder/godot/servers/rendering/rendering_device_graph.*`

use crate::engine::scene::{
    SceneRenderPassKind, SceneRenderTargetKind, SceneRenderingDeviceGraphPlan,
    SceneRenderingDeviceImageAccess,
    SceneRenderingDeviceSampledBinding, SceneRenderingDeviceTargetAllocation, SceneResourceId,
    SceneStringId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) enum SceneSampledImageSource {
    FallbackWhite,
    SceneTexture {
        resource: SceneResourceId,
    },
    SceneColorSnapshot,
    EffectTarget {
        physical_slot: u32,
        batch_atlas_tile: u32,
    },
    VideoFrame {
        media_instance: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct SceneSampledImageBindingPlan {
    pub sampled_slot_count: usize,
    pub sources: Vec<SceneSampledImageSource>,
    pub initial_reference_physical_slots: Vec<u32>,
    pub fallback_descriptor_count: usize,
    pub scene_texture_descriptor_count: usize,
    pub scene_color_snapshot_descriptor_count: usize,
    pub effect_target_descriptor_count: usize,
    pub video_frame_descriptor_count: usize,
}

impl SceneSampledImageBindingPlan {
    pub fn source(
        &self,
        draw_index: usize,
        sampled_index: usize,
    ) -> Option<SceneSampledImageSource> {
        draw_index
            .checked_mul(self.sampled_slot_count)
            .and_then(|base| base.checked_add(sampled_index))
            .and_then(|index| self.sources.get(index))
            .copied()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LogicalTargetReference {
    pub(super) graph_index: u32,
    pub(super) target: SceneRenderTargetKind,
    pub(super) target_name: SceneStringId,
    pub(super) physical_slot: u32,
}

pub(in crate::renderer::native_vulkan) fn scene_sampled_image_binding_plan(
    graph: &SceneRenderingDeviceGraphPlan,
    sampled_slots: &[u32],
    input_attachment_slots: &[u32],
) -> Result<SceneSampledImageBindingPlan, String> {
    scene_sampled_image_binding_cycle(graph, sampled_slots, input_attachment_slots)?
        .into_iter()
        .next()
        .ok_or_else(|| "scene sampled binding cycle is empty".to_owned())
}

pub(in crate::renderer::native_vulkan) fn scene_sampled_image_binding_cycle(
    graph: &SceneRenderingDeviceGraphPlan,
    sampled_slots: &[u32],
    input_attachment_slots: &[u32],
) -> Result<Vec<SceneSampledImageBindingPlan>, String> {
    let initial_references = logical_target_references(&graph.target_allocations);
    let mut references = initial_references.clone();
    let mut cycle = Vec::new();
    loop {
        if cycle.len() >= 1_024 {
            return Err("scene sampled target reference cycle exceeds 1024 frames".to_owned());
        }
        if cycle.iter().any(|plan: &SceneSampledImageBindingPlan| {
            plan.initial_reference_physical_slots == reference_physical_slots(&references)
        }) {
            return Err(
                "scene sampled target references entered a non-origin permutation cycle".to_owned(),
            );
        }
        cycle.push(scene_sampled_image_binding_plan_for_references(
            graph,
            sampled_slots,
            input_attachment_slots,
            &mut references,
        )?);
        if references == initial_references {
            break;
        }
    }
    if std::env::var_os("GILDER_NATIVE_VULKAN_SCENE_SAMPLED_BINDING_DEBUG").is_some() {
        for (phase, plan) in cycle.iter().enumerate() {
            for draw_index in 0..graph.mesh_draws.len() {
                for (sampled_index, slot) in sampled_slots.iter().copied().enumerate() {
                    let Some(source) = plan.source(draw_index, sampled_index) else {
                        continue;
                    };
                    if !matches!(source, SceneSampledImageSource::FallbackWhite) {
                        eprintln!(
                            "gilder-sampled-binding: phase={phase} draw={draw_index} slot={slot} source={source:?}"
                        );
                    }
                }
            }
        }
    }
    Ok(cycle)
}

fn scene_sampled_image_binding_plan_for_references(
    graph: &SceneRenderingDeviceGraphPlan,
    sampled_slots: &[u32],
    input_attachment_slots: &[u32],
    references: &mut [LogicalTargetReference],
) -> Result<SceneSampledImageBindingPlan, String> {
    let source_count = graph.mesh_draws.len().saturating_mul(sampled_slots.len());
    let mut sources = vec![SceneSampledImageSource::FallbackWhite; source_count];
    let initial_reference_physical_slots = reference_physical_slots(references);
    lower_material_sampled_bindings(graph, sampled_slots, &mut sources)?;

    for (pass_node_index, pass) in graph.pass_nodes.iter().enumerate() {
        let pass_bindings = graph
            .sampled_bindings
            .iter()
            .filter(|binding| binding.pass_node_index == pass_node_index as u32)
            .collect::<Vec<_>>();
        if pass.role == SceneRenderPassKind::SwapTargetReferences {
            apply_swap_reference(
                pass.graph_index,
                pass.target,
                pass.target_name,
                &pass_bindings,
                references,
            )?;
            continue;
        }
        if pass.role == SceneRenderPassKind::CopyTarget {
            continue;
        }
        lower_pass_sampled_bindings(
            graph,
            pass.mesh_draw_start,
            pass.mesh_draw_count,
            &pass_bindings,
            sampled_slots,
            input_attachment_slots,
            references,
            &mut sources,
        )?;
    }

    let effect_target_descriptor_count = sources
        .iter()
        .filter(|source| matches!(source, SceneSampledImageSource::EffectTarget { .. }))
        .count();
    let scene_texture_descriptor_count = sources
        .iter()
        .filter(|source| matches!(source, SceneSampledImageSource::SceneTexture { .. }))
        .count();
    let scene_color_snapshot_descriptor_count = sources
        .iter()
        .filter(|source| matches!(source, SceneSampledImageSource::SceneColorSnapshot))
        .count();
    let video_frame_descriptor_count = sources
        .iter()
        .filter(|source| matches!(source, SceneSampledImageSource::VideoFrame { .. }))
        .count();
    Ok(SceneSampledImageBindingPlan {
        sampled_slot_count: sampled_slots.len(),
        initial_reference_physical_slots,
        fallback_descriptor_count: sources
            .len()
            .saturating_sub(effect_target_descriptor_count)
            .saturating_sub(scene_texture_descriptor_count)
            .saturating_sub(scene_color_snapshot_descriptor_count)
            .saturating_sub(video_frame_descriptor_count),
        scene_texture_descriptor_count,
        scene_color_snapshot_descriptor_count,
        effect_target_descriptor_count,
        video_frame_descriptor_count,
        sources,
    })
}

fn lower_material_sampled_bindings(
    graph: &SceneRenderingDeviceGraphPlan,
    sampled_slots: &[u32],
    sources: &mut [SceneSampledImageSource],
) -> Result<(), String> {
    for binding in &graph.material_sampled_bindings {
        let Some(sampled_index) = sampled_slots.iter().position(|slot| *slot == binding.slot)
        else {
            continue;
        };
        let source_index = binding.draw_index as usize * sampled_slots.len() + sampled_index;
        let source = sources.get_mut(source_index).ok_or_else(|| {
            format!(
                "scene material texture references missing draw {} sampled slot {}",
                binding.draw_index, binding.slot
            )
        })?;
        if !matches!(source, SceneSampledImageSource::FallbackWhite) {
            return Err(format!(
                "scene draw {} has duplicate material texture binding for sampled slot {}",
                binding.draw_index, binding.slot
            ));
        }
        *source = SceneSampledImageSource::SceneTexture {
            resource: binding.resource,
        };
    }
    Ok(())
}

pub(super) fn reference_physical_slots(references: &[LogicalTargetReference]) -> Vec<u32> {
    references
        .iter()
        .map(|reference| reference.physical_slot)
        .collect()
}

fn lower_pass_sampled_bindings(
    graph: &SceneRenderingDeviceGraphPlan,
    draw_start: u32,
    draw_count: u32,
    bindings: &[&SceneRenderingDeviceSampledBinding],
    sampled_slots: &[u32],
    input_attachment_slots: &[u32],
    references: &[LogicalTargetReference],
    sources: &mut [SceneSampledImageSource],
) -> Result<(), String> {
    for binding in bindings {
        if binding.access == SceneRenderingDeviceImageAccess::InputAttachment {
            if !input_attachment_slots.contains(&binding.slot) {
                return Err(format!(
                    "scene input-attachment binding pass {} slot {} is absent from input-attachment shader contracts",
                    binding.pass_node_index, binding.slot
                ));
            }
            continue;
        }
        if binding.access != SceneRenderingDeviceImageAccess::SampledImage {
            return Err(format!(
                "scene image binding pass {} slot {} has an unsupported access {:?}",
                binding.pass_node_index, binding.slot, binding.access
            ));
        }
        if binding.kind == crate::engine::scene::SceneRenderBindingKind::VideoFrame {
            lower_video_frame_binding(draw_start, draw_count, binding, sampled_slots, sources)?;
            continue;
        }
        let Some((graph_index, target, target_name)) = binding.logical_target() else {
            continue;
        };
        let sampled_index = sampled_slots
            .iter()
            .position(|slot| *slot == binding.slot)
            .ok_or_else(|| {
                format!(
                    "scene graph target binding slot {} is absent from drawable shader contracts",
                    binding.slot
                )
            })?;
        let physical_slot = reference_physical_slot(references, graph_index, target, target_name)
            .ok_or_else(|| {
            format!(
                "scene graph {graph_index} pass {} target binding {:?}:{:?} at sampled slot {} has no physical allocation",
                binding.pass_node_index, target, target_name, binding.slot
            )
        })?;
        for draw_index in draw_start..draw_start.saturating_add(draw_count) {
            let source_index = draw_index as usize * sampled_slots.len() + sampled_index;
            let source = sources.get_mut(source_index).ok_or_else(|| {
                format!(
                    "scene graph target binding references missing draw {draw_index} sampled slot {}",
                    binding.slot
                )
            })?;
            if matches!(source, SceneSampledImageSource::EffectTarget { .. }) {
                return Err(format!(
                    "scene draw {draw_index} has duplicate graph target binding for sampled slot {}",
                    binding.slot
                ));
            }
            *source =
                if target_is_direct_scene_color_snapshot(graph, graph_index, target, target_name) {
                    SceneSampledImageSource::SceneColorSnapshot
                } else {
                    SceneSampledImageSource::EffectTarget {
                        physical_slot,
                        batch_atlas_tile: graph
                            .effect_batch_atlas_tile(graph_index, target, target_name)
                            .unwrap_or(0),
                    }
                };
        }
    }
    Ok(())
}

fn lower_video_frame_binding(
    draw_start: u32,
    draw_count: u32,
    binding: &SceneRenderingDeviceSampledBinding,
    sampled_slots: &[u32],
    sources: &mut [SceneSampledImageSource],
) -> Result<(), String> {
    let sampled_index = sampled_slots
        .iter()
        .position(|slot| *slot == binding.slot)
        .ok_or_else(|| {
            format!(
                "scene video frame media instance {} is absent from drawable shader contracts",
                binding.slot
            )
        })?;
    for draw_index in draw_start..draw_start.saturating_add(draw_count) {
        let source_index = draw_index as usize * sampled_slots.len() + sampled_index;
        let source = sources.get_mut(source_index).ok_or_else(|| {
            format!(
                "scene video frame binding references missing draw {draw_index} media instance {}",
                binding.slot
            )
        })?;
        if !matches!(source, SceneSampledImageSource::FallbackWhite) {
            return Err(format!(
                "scene draw {draw_index} has duplicate binding for video media instance {}",
                binding.slot
            ));
        }
        *source = SceneSampledImageSource::VideoFrame {
            media_instance: binding.slot,
        };
    }
    Ok(())
}

pub(in crate::renderer::native_vulkan) fn target_is_direct_scene_color_snapshot(
    graph: &SceneRenderingDeviceGraphPlan,
    graph_index: u32,
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
) -> bool {
    if target != SceneRenderTargetKind::FirstClassEffectTarget {
        return false;
    }
    let Some(copy_pass_index) = graph
        .pass_nodes
        .iter()
        .enumerate()
        .find_map(|(pass_index, pass)| {
            (pass.graph_index == graph_index
                && pass.role == SceneRenderPassKind::CopyTarget
                && pass.target == target
                && pass.target_name == target_name
                && graph.sampled_bindings.iter().any(|binding| {
                    binding.pass_node_index == pass_index as u32
                        && binding.target == SceneRenderTargetKind::SceneColor
                }))
            .then_some(pass_index)
        })
    else {
        return false;
    };
    let mut consumers = graph
        .sampled_bindings
        .iter()
        .filter(|binding| {
            binding.graph_index == graph_index
                && binding.target == target
                && binding.target_name == target_name
        })
        .peekable();
    consumers.peek().is_some()
        && consumers.all(|binding| {
            let consumer_index = binding.pass_node_index as usize;
            consumer_index > copy_pass_index
                && graph
                    .pass_nodes
                    .get(copy_pass_index + 1..=consumer_index)
                    .is_some_and(|passes| {
                        passes.iter().all(|pass| {
                            pass.graph_index == graph_index
                                && pass.target != SceneRenderTargetKind::SceneColor
                        })
                    })
        })
}

pub(super) fn apply_swap_reference(
    graph_index: u32,
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
    bindings: &[&SceneRenderingDeviceSampledBinding],
    references: &mut [LogicalTargetReference],
) -> Result<(), String> {
    let (source_graph_index, source_target, source_name) = bindings
        .iter()
        .find_map(|binding| binding.logical_target())
        .ok_or_else(|| "scene effect swap pass has no logical source target binding".to_owned())?;
    let source_index = reference_index(references, source_graph_index, source_target, source_name)
        .ok_or_else(|| "scene effect swap source target has no physical allocation".to_owned())?;
    let target_index =
        reference_index(references, graph_index, target, target_name).ok_or_else(|| {
            "scene effect swap destination target has no physical allocation".to_owned()
        })?;
    let source_physical_slot = references[source_index].physical_slot;
    references[source_index].physical_slot = references[target_index].physical_slot;
    references[target_index].physical_slot = source_physical_slot;
    Ok(())
}

pub(super) fn logical_target_references(
    allocations: &[SceneRenderingDeviceTargetAllocation],
) -> Vec<LogicalTargetReference> {
    allocations
        .iter()
        .map(|allocation| LogicalTargetReference {
            graph_index: allocation.graph_index,
            target: allocation.target,
            target_name: allocation.target_name,
            physical_slot: allocation.physical_slot,
        })
        .collect()
}

pub(super) fn reference_physical_slot(
    references: &[LogicalTargetReference],
    graph_index: u32,
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
) -> Option<u32> {
    reference_index(references, graph_index, target, target_name)
        .map(|index| references[index].physical_slot)
}

pub(super) fn reference_index(
    references: &[LogicalTargetReference],
    graph_index: u32,
    target: SceneRenderTargetKind,
    target_name: SceneStringId,
) -> Option<usize> {
    references.iter().position(|reference| {
        reference.graph_index == graph_index
            && reference.target == target
            && reference.target_name == target_name
    })
}

#[cfg(test)]
#[path = "sampled_binding/tests.rs"]
mod tests;
