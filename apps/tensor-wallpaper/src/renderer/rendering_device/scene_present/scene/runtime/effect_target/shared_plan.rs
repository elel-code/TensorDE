//! Phase-aware lowering of logical effect commands to physical shared images.

use super::*;
use crate::renderer::rendering_device::scene_present::scene::runtime::local_read::{
    SceneLocalReadScopePassRole, SceneLocalReadScopePlan,
};
use crate::renderer::rendering_device::scene_present::scene::runtime::sampled_binding::SceneSampledImageBindingPlan;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::rendering_device::scene_present::scene::runtime) enum SharedSceneEffectCopySource {
    SceneColor,
    PhysicalSlot(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::rendering_device::scene_present::scene::runtime) enum SharedSceneEffectLoadOp {
    Load,
    Clear,
    Discard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::rendering_device::scene_present::scene::runtime) enum SharedSceneEffectCommandKind {
    Copy {
        source: SharedSceneEffectCopySource,
        destination_physical_slot: Option<u32>,
        direct_scene_color_snapshot: bool,
        coverage: SceneColorCopyCoverage,
    },
    SwapReferences {
        source_before: u32,
        destination_before: u32,
    },
    DynamicRender {
        target_physical_slot: u32,
        draw_start: u32,
        draw_count: u32,
        load_op: SharedSceneEffectLoadOp,
        batch_physical_slot: Option<u32>,
        batch_atlas_tile: Option<u32>,
        local_read: Option<(usize, SceneLocalReadScopePassRole)>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::rendering_device::scene_present::scene::runtime) struct SharedSceneEffectCommand {
    pub source_position: usize,
    pub graph_index: u32,
    pub pass_record_index: u32,
    pub kind: SharedSceneEffectCommandKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::rendering_device::scene_present::scene::runtime) struct SharedSceneEffectExecutionPlan
{
    pub reference_phase: usize,
    pub commands: Vec<SharedSceneEffectCommand>,
}

pub(in crate::renderer::rendering_device::scene_present::scene::runtime) fn shared_scene_effect_execution_plans(
    commands: &[SceneEffectTargetCommand],
    allocations: &[SceneRenderingDeviceTargetAllocation],
    image_plans: &[SceneEffectTargetImagePlan],
    reference_phases: &[SceneSampledImageBindingPlan],
    local_read_scopes: &[SceneLocalReadScopePlan],
) -> Result<Vec<SharedSceneEffectExecutionPlan>, String> {
    reference_phases
        .iter()
        .enumerate()
        .map(|(reference_phase, phase)| {
            compile_phase(
                reference_phase,
                commands,
                allocations,
                image_plans,
                &phase.initial_reference_physical_slots,
                local_read_scopes,
            )
        })
        .collect()
}

fn compile_phase(
    reference_phase: usize,
    commands: &[SceneEffectTargetCommand],
    allocations: &[SceneRenderingDeviceTargetAllocation],
    image_plans: &[SceneEffectTargetImagePlan],
    initial_reference_physical_slots: &[u32],
    local_read_scopes: &[SceneLocalReadScopePlan],
) -> Result<SharedSceneEffectExecutionPlan, String> {
    let mut references = logical_target_references(allocations);
    if references.len() != initial_reference_physical_slots.len() {
        return Err(format!(
            "shared effect reference phase {reference_phase} has {} slots for {} logical targets",
            initial_reference_physical_slots.len(),
            references.len()
        ));
    }
    for (reference, physical_slot) in references
        .iter_mut()
        .zip(initial_reference_physical_slots.iter().copied())
    {
        reference.physical_slot = physical_slot;
    }
    let mut initialized_logical_targets = references
        .iter()
        .filter(|reference| {
            image_plans.iter().any(|plan| {
                plan.physical_slot == reference.physical_slot && plan.persistent_across_frames
            })
        })
        .map(|reference| reference.key)
        .collect::<Vec<_>>();

    let mut compiled = Vec::with_capacity(commands.len());
    for (source_position, command) in commands.iter().copied().enumerate() {
        let graph_index = command.target.graph_index;
        let kind = match command.kind {
            SceneEffectTargetCommandKind::Copy => {
                let source = match command.source {
                    Some(SceneEffectTargetCommandSource::SceneColor) => {
                        SharedSceneEffectCopySource::SceneColor
                    }
                    Some(SceneEffectTargetCommandSource::LogicalTarget(source)) => {
                        SharedSceneEffectCopySource::PhysicalSlot(physical_slot(
                            &references,
                            source,
                            "copy source",
                        )?)
                    }
                    None => return Err("shared effect copy command has no source".into()),
                };
                let destination_physical_slot = (!command.direct_scene_color_snapshot)
                    .then(|| physical_slot(&references, command.target, "copy destination"))
                    .transpose()?;
                SharedSceneEffectCommandKind::Copy {
                    source,
                    destination_physical_slot,
                    direct_scene_color_snapshot: command.direct_scene_color_snapshot,
                    coverage: command.scene_color_copy_coverage,
                }
            }
            SceneEffectTargetCommandKind::SwapReferences => {
                let source_key = command
                    .source
                    .and_then(|source| match source {
                        SceneEffectTargetCommandSource::LogicalTarget(key) => Some(key),
                        SceneEffectTargetCommandSource::SceneColor => None,
                    })
                    .ok_or_else(|| {
                        "shared effect swap command has no logical source target".to_owned()
                    })?;
                let source_before = physical_slot(&references, source_key, "swap source")?;
                let destination_before =
                    physical_slot(&references, command.target, "swap destination")?;
                swap_logical_references(command, &mut references)?;
                for target in [source_key, command.target] {
                    let slot = physical_slot(&references, target, "swapped target")?;
                    if image_plans.iter().any(|plan| plan.physical_slot == slot) {
                        mark_initialized(&mut initialized_logical_targets, target);
                    }
                }
                SharedSceneEffectCommandKind::SwapReferences {
                    source_before,
                    destination_before,
                }
            }
            SceneEffectTargetCommandKind::DynamicRender => {
                let load_op = if command.clear_before_draw {
                    SharedSceneEffectLoadOp::Clear
                } else if command.fully_overwrites_target {
                    SharedSceneEffectLoadOp::Discard
                } else if initialized_logical_targets.contains(&command.target) {
                    SharedSceneEffectLoadOp::Load
                } else {
                    SharedSceneEffectLoadOp::Clear
                };
                let local_read =
                    local_read_scopes
                        .iter()
                        .enumerate()
                        .find_map(|(scope_index, scope)| {
                            if local_read_scope_matches_command(scope, &command, true) {
                                Some((scope_index, SceneLocalReadScopePassRole::Producer))
                            } else if local_read_scope_matches_command(scope, &command, false) {
                                Some((scope_index, SceneLocalReadScopePassRole::Consumer))
                            } else {
                                None
                            }
                        });
                SharedSceneEffectCommandKind::DynamicRender {
                    target_physical_slot: physical_slot(
                        &references,
                        command.target,
                        "dynamic-render target",
                    )?,
                    draw_start: command.mesh_draw_start,
                    draw_count: command.mesh_draw_count,
                    load_op,
                    batch_physical_slot: command.batch_physical_slot,
                    batch_atlas_tile: command.batch_atlas_tile,
                    local_read,
                }
            }
        };
        if matches!(
            command.kind,
            SceneEffectTargetCommandKind::Copy | SceneEffectTargetCommandKind::DynamicRender
        ) {
            mark_initialized(&mut initialized_logical_targets, command.target);
        }
        compiled.push(SharedSceneEffectCommand {
            source_position,
            graph_index,
            pass_record_index: command.pass_record_index,
            kind,
        });
    }
    validate_local_read_pairs(&compiled, local_read_scopes.len())?;
    Ok(SharedSceneEffectExecutionPlan {
        reference_phase,
        commands: compiled,
    })
}

fn mark_initialized(
    initialized_logical_targets: &mut Vec<LogicalEffectTargetKey>,
    target: LogicalEffectTargetKey,
) {
    if !initialized_logical_targets.contains(&target) {
        initialized_logical_targets.push(target);
    }
}

fn physical_slot(
    references: &[LogicalEffectTargetReference],
    key: LogicalEffectTargetKey,
    role: &str,
) -> Result<u32, String> {
    references
        .iter()
        .find(|reference| reference.key == key)
        .map(|reference| reference.physical_slot)
        .ok_or_else(|| format!("shared effect {role} is not allocated"))
}

fn validate_local_read_pairs(
    commands: &[SharedSceneEffectCommand],
    scope_count: usize,
) -> Result<(), String> {
    for scope_index in 0..scope_count {
        let roles = commands
            .iter()
            .filter_map(|command| match command.kind {
                SharedSceneEffectCommandKind::DynamicRender {
                    local_read: Some((candidate, role)),
                    ..
                } if candidate == scope_index => Some((command.source_position, role)),
                _ => None,
            })
            .collect::<Vec<_>>();
        if roles.len() != 2
            || roles[0].1 != SceneLocalReadScopePassRole::Producer
            || roles[1].1 != SceneLocalReadScopePassRole::Consumer
            || roles[1].0 != roles[0].0 + 1
        {
            return Err(format!(
                "shared local-read scope {scope_index} does not lower to adjacent producer/consumer commands"
            ));
        }
    }
    Ok(())
}
