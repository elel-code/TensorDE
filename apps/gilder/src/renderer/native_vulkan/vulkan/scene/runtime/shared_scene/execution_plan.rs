//! Cold-compiled allocation-free authored frame execution schedule.

use crate::engine::scene::{
    SceneRenderGraphActivationPolicy, SceneRenderPassKind, SceneRenderTargetKind,
    SceneRenderingDeviceGraphPlan, SceneRenderingDevicePassNode,
};

use super::super::draw_recording::SceneGpuDrawRange;
use super::super::effect_target::{
    SceneEffectTargetImagePlan, SharedSceneEffectCommand, SharedSceneEffectCommandKind,
    SharedSceneEffectExecutionPlan,
};
use super::super::local_read::SceneLocalReadScopePassRole;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SharedSceneFrameStep {
    SceneColor(SceneGpuDrawRange),
    Effect {
        command_index: usize,
    },
    LocalReadPair {
        producer_command_index: usize,
        consumer_command_index: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SharedSceneGraphExecution {
    pub graph_index: u32,
    pub activation_policy: SceneRenderGraphActivationPolicy,
    pub draw_ranges: Vec<SceneGpuDrawRange>,
    pub steps: Vec<SharedSceneFrameStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SharedSceneEffectBatchExecution {
    pub physical_slot: u32,
    pub command_indices: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SharedSceneFrameExecutionPlan {
    pub graphs: Vec<SharedSceneGraphExecution>,
    pub effect_batches: Vec<SharedSceneEffectBatchExecution>,
}

pub(super) fn compile_shared_scene_frame_execution_plan(
    graph: &SceneRenderingDeviceGraphPlan,
    graph_order: &[u32],
    effect_targets: &[SceneEffectTargetImagePlan],
    phases: &[SharedSceneEffectExecutionPlan],
) -> Result<SharedSceneFrameExecutionPlan, String> {
    let first_phase = phases
        .first()
        .ok_or_else(|| "shared frame execution requires an effect reference phase".to_owned())?;
    validate_phase_topology(first_phase, phases)?;
    let effect_batches = compile_batches(effect_targets, first_phase)?;
    let mut command_cursor = 0usize;
    let mut graphs = Vec::with_capacity(graph_order.len());
    for graph_index in graph_order {
        let passes = graph
            .pass_nodes
            .iter()
            .filter(|pass| pass.graph_index == *graph_index)
            .collect::<Vec<_>>();
        let activation_policy = passes
            .first()
            .map(|pass| pass.graph_activation_policy)
            .unwrap_or(SceneRenderGraphActivationPolicy::Always);
        let draw_ranges = passes
            .iter()
            .filter(|pass| pass.mesh_draw_count != 0)
            .map(|pass| SceneGpuDrawRange {
                start: pass.mesh_draw_start,
                count: pass.mesh_draw_count,
            })
            .collect();
        let mut steps = Vec::with_capacity(passes.len());
        let mut pass_cursor = 0usize;
        while let Some(pass) = passes.get(pass_cursor).copied() {
            if pass_targets_scene_color(pass) {
                if pass.mesh_draw_count != 0 {
                    steps.push(SharedSceneFrameStep::SceneColor(SceneGpuDrawRange {
                        start: pass.mesh_draw_start,
                        count: pass.mesh_draw_count,
                    }));
                }
                pass_cursor += 1;
                continue;
            }
            let command_index =
                next_command_index(first_phase, *graph_index, pass, &mut command_cursor)?;
            let command = first_phase.commands[command_index];
            if matches!(
                command.kind,
                SharedSceneEffectCommandKind::DynamicRender {
                    local_read: Some((_, SceneLocalReadScopePassRole::Producer)),
                    ..
                }
            ) {
                let consumer_pass = passes.get(pass_cursor + 1).copied().ok_or_else(|| {
                    format!(
                        "shared graph {graph_index} local-read producer has no authored consumer"
                    )
                })?;
                let consumer_command_index = next_command_index(
                    first_phase,
                    *graph_index,
                    consumer_pass,
                    &mut command_cursor,
                )?;
                validate_local_pair(command, first_phase.commands[consumer_command_index])?;
                steps.push(SharedSceneFrameStep::LocalReadPair {
                    producer_command_index: command_index,
                    consumer_command_index,
                });
                pass_cursor += 2;
                continue;
            }
            if matches!(
                command.kind,
                SharedSceneEffectCommandKind::DynamicRender {
                    local_read: Some((_, SceneLocalReadScopePassRole::Consumer)),
                    ..
                }
            ) {
                return Err(format!(
                    "shared graph {graph_index} starts a retained step with a local-read consumer"
                ));
            }
            steps.push(SharedSceneFrameStep::Effect { command_index });
            pass_cursor += 1;
        }
        graphs.push(SharedSceneGraphExecution {
            graph_index: *graph_index,
            activation_policy,
            draw_ranges,
            steps,
        });
    }
    while first_phase
        .commands
        .get(command_cursor)
        .is_some_and(is_batch_command)
    {
        command_cursor += 1;
    }
    if command_cursor != first_phase.commands.len() {
        return Err(format!(
            "shared frame schedule consumed {command_cursor} of {} effect commands",
            first_phase.commands.len()
        ));
    }
    Ok(SharedSceneFrameExecutionPlan {
        graphs,
        effect_batches,
    })
}

fn compile_batches(
    targets: &[SceneEffectTargetImagePlan],
    phase: &SharedSceneEffectExecutionPlan,
) -> Result<Vec<SharedSceneEffectBatchExecution>, String> {
    let mut batches = Vec::new();
    for target in targets.iter().filter(|target| target.batch_field_count > 1) {
        let mut tiles = Vec::new();
        let mut command_indices = Vec::new();
        for (index, command) in phase.commands.iter().enumerate() {
            let SharedSceneEffectCommandKind::DynamicRender {
                batch_physical_slot: Some(physical_slot),
                batch_atlas_tile: Some(tile),
                ..
            } = command.kind
            else {
                continue;
            };
            if physical_slot == target.physical_slot && !tiles.contains(&tile) {
                tiles.push(tile);
                command_indices.push(index);
            }
        }
        if command_indices.len() > target.batch_field_count as usize {
            return Err(format!(
                "shared effect batch slot {} has {} unique tiles for {} fields",
                target.physical_slot,
                command_indices.len(),
                target.batch_field_count
            ));
        }
        if !command_indices.is_empty() {
            batches.push(SharedSceneEffectBatchExecution {
                physical_slot: target.physical_slot,
                command_indices,
            });
        }
    }
    Ok(batches)
}

fn next_command_index(
    phase: &SharedSceneEffectExecutionPlan,
    graph_index: u32,
    pass: &SceneRenderingDevicePassNode,
    cursor: &mut usize,
) -> Result<usize, String> {
    while let Some(command) = phase.commands.get(*cursor).copied() {
        let index = *cursor;
        *cursor += 1;
        if is_batch_command(&command) {
            continue;
        }
        if command.graph_index != graph_index || !command_matches_pass(command, pass) {
            return Err(format!(
                "shared effect command {index} does not match graph {graph_index} pass {}",
                pass.pass_id
            ));
        }
        return Ok(index);
    }
    Err(format!(
        "shared graph {graph_index} pass {} has no effect command",
        pass.pass_id
    ))
}

fn command_matches_pass(
    command: SharedSceneEffectCommand,
    pass: &SceneRenderingDevicePassNode,
) -> bool {
    if command.pass_record_index != pass.pass_record_index {
        return false;
    }
    match command.kind {
        SharedSceneEffectCommandKind::Copy { .. } => pass.role == SceneRenderPassKind::CopyTarget,
        SharedSceneEffectCommandKind::SwapReferences { .. } => {
            pass.role == SceneRenderPassKind::SwapTargetReferences
        }
        SharedSceneEffectCommandKind::DynamicRender {
            draw_start,
            draw_count,
            ..
        } => (draw_start, draw_count) == (pass.mesh_draw_start, pass.mesh_draw_count),
    }
}

fn validate_local_pair(
    producer: SharedSceneEffectCommand,
    consumer: SharedSceneEffectCommand,
) -> Result<(), String> {
    match (producer.kind, consumer.kind) {
        (
            SharedSceneEffectCommandKind::DynamicRender {
                local_read: Some((producer_scope, SceneLocalReadScopePassRole::Producer)),
                ..
            },
            SharedSceneEffectCommandKind::DynamicRender {
                local_read: Some((consumer_scope, SceneLocalReadScopePassRole::Consumer)),
                ..
            },
        ) if producer_scope == consumer_scope => Ok(()),
        _ => Err("shared frame schedule local-read commands are not a matched pair".into()),
    }
}

fn validate_phase_topology(
    first: &SharedSceneEffectExecutionPlan,
    phases: &[SharedSceneEffectExecutionPlan],
) -> Result<(), String> {
    for phase in phases {
        if phase.commands.len() != first.commands.len()
            || phase
                .commands
                .iter()
                .zip(&first.commands)
                .any(|(candidate, expected)| command_shape(*candidate) != command_shape(*expected))
        {
            return Err(format!(
                "shared effect phase {} changes authored command topology",
                phase.reference_phase
            ));
        }
    }
    Ok(())
}

fn command_shape(command: SharedSceneEffectCommand) -> (u32, u32, u8, Option<(usize, u8)>, bool) {
    let (kind, local, batch) = match command.kind {
        SharedSceneEffectCommandKind::Copy { .. } => (0, None, false),
        SharedSceneEffectCommandKind::SwapReferences { .. } => (1, None, false),
        SharedSceneEffectCommandKind::DynamicRender {
            local_read,
            batch_atlas_tile,
            ..
        } => (
            2,
            local_read.map(|(scope, role)| {
                (
                    scope,
                    match role {
                        SceneLocalReadScopePassRole::Producer => 0,
                        SceneLocalReadScopePassRole::Consumer => 1,
                    },
                )
            }),
            batch_atlas_tile.is_some(),
        ),
    };
    (
        command.graph_index,
        command.pass_record_index,
        kind,
        local,
        batch,
    )
}

fn is_batch_command(command: &SharedSceneEffectCommand) -> bool {
    matches!(
        command.kind,
        SharedSceneEffectCommandKind::DynamicRender {
            batch_atlas_tile: Some(_),
            ..
        }
    )
}

fn pass_targets_scene_color(pass: &SceneRenderingDevicePassNode) -> bool {
    matches!(
        pass.target,
        SceneRenderTargetKind::SceneColor | SceneRenderTargetKind::Swapchain
    )
}
