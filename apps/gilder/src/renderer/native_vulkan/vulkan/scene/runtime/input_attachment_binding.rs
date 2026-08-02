//! Typed per-draw input-attachment lowering for scene graph targets.
//!
//! This module deliberately has no sampled-image or sampler fallback.  A
//! declared input-attachment slot must resolve to a retained effect-target
//! image for every draw whose shader contract consumes it.

use crate::engine::scene::{
    SceneRenderPassKind, SceneRenderTargetKind, SceneRenderingDeviceGraphPlan,
    SceneRenderingDeviceImageAccess, SceneStorage,
};

use super::sampled_binding::{
    LogicalTargetReference, SceneSampledImageBindingPlan, apply_swap_reference,
    logical_target_references, reference_physical_slot,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) enum SceneInputAttachmentSource {
    EffectTarget {
        physical_slot: u32,
        batch_atlas_tile: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct SceneInputAttachmentBindingPlan {
    pub input_attachment_slot_count: usize,
    pub sources: Vec<Option<SceneInputAttachmentSource>>,
    pub effect_target_descriptor_count: usize,
}

impl SceneInputAttachmentBindingPlan {
    pub(in crate::renderer::native_vulkan) fn source(
        &self,
        draw_index: usize,
        input_index: usize,
    ) -> Option<SceneInputAttachmentSource> {
        draw_index
            .checked_mul(self.input_attachment_slot_count)
            .and_then(|base| base.checked_add(input_index))
            .and_then(|index| self.sources.get(index))
            .copied()
            .flatten()
    }
}

pub(in crate::renderer::native_vulkan) fn scene_input_attachment_binding_cycle(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    input_attachment_slots: &[u32],
    sampled_binding_cycle: &[SceneSampledImageBindingPlan],
) -> Result<Vec<SceneInputAttachmentBindingPlan>, String> {
    if sampled_binding_cycle.is_empty() {
        return Err("scene input-attachment binding cycle has no sampled phase anchor".to_owned());
    }
    sampled_binding_cycle
        .iter()
        .map(|sampled_phase| {
            scene_input_attachment_binding_plan_for_phase(
                storage,
                graph,
                input_attachment_slots,
                sampled_phase,
            )
        })
        .collect()
}

fn scene_input_attachment_binding_plan_for_phase(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    input_attachment_slots: &[u32],
    sampled_phase: &SceneSampledImageBindingPlan,
) -> Result<SceneInputAttachmentBindingPlan, String> {
    let source_count = graph
        .mesh_draws
        .len()
        .saturating_mul(input_attachment_slots.len());
    let mut sources = vec![None; source_count];
    if input_attachment_slots.is_empty() {
        return Ok(SceneInputAttachmentBindingPlan {
            input_attachment_slot_count: 0,
            sources,
            effect_target_descriptor_count: 0,
        });
    }

    let mut references = logical_target_references(&graph.target_allocations);
    apply_phase_reference_slots(&mut references, sampled_phase)?;
    for (pass_node_index, pass) in graph.pass_nodes.iter().enumerate() {
        let pass_bindings = graph
            .sampled_bindings
            .iter()
            .filter(|binding| binding.pass_node_index == pass_node_index as u32)
            .collect::<Vec<_>>();
        if pass.role == SceneRenderPassKind::SwapTargetReferences {
            if pass_bindings
                .iter()
                .any(|binding| binding.access == SceneRenderingDeviceImageAccess::InputAttachment)
            {
                return Err(format!(
                    "scene swap pass {} cannot declare an input-attachment binding",
                    pass.pass_id
                ));
            }
            apply_swap_reference(
                pass.graph_index,
                pass.target,
                pass.target_name,
                &pass_bindings,
                &mut references,
            )?;
            continue;
        }
        if pass.role == SceneRenderPassKind::CopyTarget {
            if pass_bindings
                .iter()
                .any(|binding| binding.access == SceneRenderingDeviceImageAccess::InputAttachment)
            {
                return Err(format!(
                    "scene copy pass {} cannot declare an input-attachment binding",
                    pass.pass_id
                ));
            }
            continue;
        }
        for binding in pass_bindings {
            if binding.access == SceneRenderingDeviceImageAccess::SampledImage {
                continue;
            }
            if binding.access != SceneRenderingDeviceImageAccess::InputAttachment {
                return Err(format!(
                    "scene image binding pass {} slot {} has unsupported input access {:?}",
                    binding.pass_node_index, binding.slot, binding.access
                ));
            }
            let input_index = input_attachment_slots
                .iter()
                .position(|slot| *slot == binding.slot)
                .ok_or_else(|| {
                    format!(
                        "scene input-attachment binding pass {} slot {} is absent from shader contracts",
                        binding.pass_node_index, binding.slot
                    )
                })?;
            if binding.kind == crate::engine::scene::SceneRenderBindingKind::VideoFrame {
                return Err(format!(
                    "scene input-attachment binding pass {} slot {} cannot consume a video frame",
                    binding.pass_node_index, binding.slot
                ));
            }
            let Some((graph_index, target, target_name)) = binding.logical_target() else {
                return Err(format!(
                    "scene input-attachment binding pass {} slot {} has no logical target",
                    binding.pass_node_index, binding.slot
                ));
            };
            if matches!(
                target,
                SceneRenderTargetKind::SceneColor
                    | SceneRenderTargetKind::Swapchain
                    | SceneRenderTargetKind::VideoExternalImage
            ) {
                return Err(format!(
                    "scene input-attachment binding pass {} slot {} targets unsupported {:?}",
                    binding.pass_node_index, binding.slot, target
                ));
            }
            let batch_atlas_tile = graph
                .effect_batch_atlas_tile(graph_index, target, target_name)
                .unwrap_or(0);
            if batch_atlas_tile != 0 {
                return Err(format!(
                    "scene input-attachment binding pass {} slot {} requires nonzero atlas tile {}",
                    binding.pass_node_index, binding.slot, batch_atlas_tile
                ));
            }
            let physical_slot =
                reference_physical_slot(&references, graph_index, target, target_name).ok_or_else(
                    || {
                        format!(
                            "scene input-attachment binding {:?}:{:?} has no physical allocation",
                            target, target_name
                        )
                    },
                )?;
            for draw_index in
                pass.mesh_draw_start..pass.mesh_draw_start.saturating_add(pass.mesh_draw_count)
            {
                let source_index = draw_index as usize * input_attachment_slots.len() + input_index;
                let source = sources.get_mut(source_index).ok_or_else(|| {
                    format!(
                        "scene input-attachment binding references missing draw {draw_index} slot {}",
                        binding.slot
                    )
                })?;
                if source.is_some() {
                    return Err(format!(
                        "scene draw {draw_index} has duplicate input-attachment binding for slot {}",
                        binding.slot
                    ));
                }
                *source = Some(SceneInputAttachmentSource::EffectTarget {
                    physical_slot,
                    batch_atlas_tile,
                });
            }
        }
    }
    validate_required_shader_slots(storage, graph, input_attachment_slots, &sources)?;
    let effect_target_descriptor_count = sources.iter().flatten().count();
    Ok(SceneInputAttachmentBindingPlan {
        input_attachment_slot_count: input_attachment_slots.len(),
        sources,
        effect_target_descriptor_count,
    })
}

fn apply_phase_reference_slots(
    references: &mut [LogicalTargetReference],
    sampled_phase: &SceneSampledImageBindingPlan,
) -> Result<(), String> {
    if references.len() != sampled_phase.initial_reference_physical_slots.len() {
        return Err(format!(
            "scene input-attachment phase has {} target references but sampled phase has {}",
            references.len(),
            sampled_phase.initial_reference_physical_slots.len()
        ));
    }
    for (reference, physical_slot) in references
        .iter_mut()
        .zip(&sampled_phase.initial_reference_physical_slots)
    {
        reference.physical_slot = *physical_slot;
    }
    Ok(())
}

fn validate_required_shader_slots(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    input_attachment_slots: &[u32],
    sources: &[Option<SceneInputAttachmentSource>],
) -> Result<(), String> {
    for (draw_index, draw) in graph.mesh_draws.iter().enumerate() {
        let Some(contract) = storage
            .shader_contracts()
            .iter()
            .find(|contract| contract.shader_key == draw.shader_key)
        else {
            if draw.shader_key == crate::engine::scene::SceneStringId::NONE {
                continue;
            }
            return Err(format!(
                "scene draw {draw_index} shader {:?} has no shader contract for input attachments",
                draw.shader_key
            ));
        };
        for (input_index, slot) in input_attachment_slots.iter().copied().enumerate() {
            let source_index = draw_index * input_attachment_slots.len() + input_index;
            let source_present = sources.get(source_index).and_then(Option::as_ref).is_some();
            if contract.input_attachment_slot_mask & (1u32 << slot) == 0 {
                if source_present {
                    return Err(format!(
                        "scene draw {draw_index} has an input-attachment source for undeclared shader slot {slot}"
                    ));
                }
                continue;
            }
            if !source_present {
                return Err(format!(
                    "scene draw {draw_index} shader slot {slot} requires an input attachment but has no graph source"
                ));
            }
        }
    }
    Ok(())
}
