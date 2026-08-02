//! Backend-neutral scene effect-target plans consumed by the shared renderer.
//!
//! References:
//! - `docs/gilder/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/gilder/docs/effect-format.md`
//! - `reverse-engineered/gilder/docs/exe/composelayer-and-effecttarget.md`
//! - `references/gilder/godot/servers/rendering/rendering_device_graph.*`

use vulkan_renderer::{Extent2D, TextureFormat};

use crate::engine::scene::{
    SceneImageTargetRecord, ScenePipelineBlend, SceneRenderPassKind, SceneRenderTargetKind,
    SceneRenderingDeviceDrawPrimitive, SceneRenderingDeviceGraphPlan,
    SceneRenderingDeviceImageAccess, SceneRenderingDevicePassNode,
    SceneRenderingDeviceTargetAllocation, SceneStorage, SceneStringId,
};

mod local_read_usage;
mod planning;
mod scene_color_copy;
mod shared_plan;

use planning::*;
pub(in crate::renderer::native_vulkan) use local_read_usage::{
    apply_scene_effect_target_input_attachment_usage,
    apply_scene_effect_target_local_read_candidate_usage,
    apply_scene_effect_target_local_read_scope_usage,
};
pub(super) use scene_color_copy::SceneColorCopyCoverage;
use scene_color_copy::scene_color_copy_coverage;
pub(super) use shared_plan::{
    SharedSceneEffectCommand, SharedSceneEffectCommandKind, SharedSceneEffectCopySource,
    SharedSceneEffectExecutionPlan, SharedSceneEffectLoadOp, shared_scene_effect_execution_plans,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct SceneEffectTargetImagePlan {
    pub physical_slot: u32,
    pub graph_index: u32,
    pub target: SceneRenderTargetKind,
    pub target_name: SceneStringId,
    pub format: TextureFormat,
    pub extent: Extent2D,
    pub batch_field_count: u32,
    pub batch_atlas_columns: u32,
    pub batch_atlas_rows: u32,
    pub persistent_across_frames: bool,
    pub aliased_logical_target_count: u32,
    pub input_attachment_required: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct SceneEffectTargetCommandPlan {
    pub dynamic_rendering_pass_count: usize,
    pub copy_command_count: usize,
    pub swap_reference_command_count: usize,
    pub mesh_draw_count: usize,
    pub fullscreen_triangle_draw_count: usize,
    pub discard_load_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct SceneEffectTargetCommand {
    kind: SceneEffectTargetCommandKind,
    pass_record_index: u32,
    target: LogicalEffectTargetKey,
    source: Option<SceneEffectTargetCommandSource>,
    mesh_draw_start: u32,
    mesh_draw_count: u32,
    clear_before_draw: bool,
    fully_overwrites_target: bool,
    direct_scene_color_snapshot: bool,
    scene_color_copy_coverage: SceneColorCopyCoverage,
    batch_physical_slot: Option<u32>,
    batch_atlas_tile: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SceneEffectTargetCommandKind {
    DynamicRender,
    Copy,
    SwapReferences,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SceneEffectTargetCommandSource {
    LogicalTarget(LogicalEffectTargetKey),
    SceneColor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct SceneEffectTargetTimingCommand {
    pub source_position: usize,
    pub graph_index: u32,
    pub graph_command_index: u32,
    pub command_kind: &'static str,
}

pub(in crate::renderer::native_vulkan) fn scene_effect_target_timing_commands(
    commands: &[SceneEffectTargetCommand],
    graph_indices: &[u32],
) -> Vec<SceneEffectTargetTimingCommand> {
    let mut timing_commands = Vec::new();
    for (source_position, command) in commands.iter().enumerate() {
        if command.batch_atlas_tile.is_some()
            || !graph_indices.contains(&command.target.graph_index)
            || command.kind == SceneEffectTargetCommandKind::SwapReferences
        {
            continue;
        }
        let graph_command_index = timing_commands
            .iter()
            .filter(|timing: &&SceneEffectTargetTimingCommand| {
                timing.graph_index == command.target.graph_index
            })
            .count() as u32;
        timing_commands.push(SceneEffectTargetTimingCommand {
            source_position,
            graph_index: command.target.graph_index,
            graph_command_index,
            command_kind: match command.kind {
                SceneEffectTargetCommandKind::Copy => "copy",
                SceneEffectTargetCommandKind::DynamicRender => "dynamic-render",
                SceneEffectTargetCommandKind::SwapReferences => unreachable!(),
            },
        });
    }
    timing_commands
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct LogicalEffectTargetKey {
    graph_index: u32,
    target: SceneRenderTargetKind,
    name: SceneStringId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LogicalEffectTargetReference {
    key: LogicalEffectTargetKey,
    physical_slot: u32,
}

pub(in crate::renderer::native_vulkan) fn scene_effect_target_image_plan(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    swapchain_format: TextureFormat,
    swapchain_extent: Extent2D,
) -> Result<Vec<SceneEffectTargetImagePlan>, String> {
    let mut allocations = graph.target_allocations.clone();
    allocations.sort_by_key(|allocation| allocation.physical_slot);
    let mut plans = Vec::<SceneEffectTargetImagePlan>::new();
    for allocation in allocations {
        if super::sampled_binding::target_is_direct_scene_color_snapshot(
            graph,
            allocation.graph_index,
            allocation.target,
            allocation.target_name,
        ) {
            continue;
        }
        let mut spec = target_spec(
            storage,
            graph,
            allocation,
            swapchain_format,
            swapchain_extent,
        )?;
        spec.batch_field_count = graph.effect_batch_field_count(allocation.physical_slot);
        [spec.batch_atlas_columns, spec.batch_atlas_rows] =
            graph.effect_batch_atlas_grid(allocation.physical_slot);
        if spec.batch_field_count > 1 {
            let divisor = u64::from(
                graph.effect_batch_field_extent_divisor(allocation.physical_slot),
            );
            spec.extent.width = (u64::from(spec.extent.width)
                .div_ceil(divisor)
                .saturating_mul(u64::from(spec.batch_atlas_columns))
                .min(u64::from(u32::MAX)) as u32)
                .max(spec.batch_atlas_columns);
            spec.extent.height = (u64::from(spec.extent.height)
                .div_ceil(divisor)
                .saturating_mul(u64::from(spec.batch_atlas_rows))
                .min(u64::from(u32::MAX)) as u32)
                .max(spec.batch_atlas_rows);
        }
        if let Some(plan) = plans
            .iter_mut()
            .find(|plan| plan.physical_slot == allocation.physical_slot)
        {
            if (plan.format, plan.persistent_across_frames)
                != (spec.format, spec.persistent_across_frames)
            {
                return Err(format!(
                    "scene effect target physical slot {} aliases incompatible images",
                    allocation.physical_slot
                ));
            }
            plan.extent.width = plan.extent.width.max(spec.extent.width);
            plan.extent.height = plan.extent.height.max(spec.extent.height);
            plan.batch_field_count = plan.batch_field_count.max(spec.batch_field_count);
            plan.batch_atlas_columns = plan.batch_atlas_columns.max(spec.batch_atlas_columns);
            plan.batch_atlas_rows = plan.batch_atlas_rows.max(spec.batch_atlas_rows);
            plan.aliased_logical_target_count = plan.aliased_logical_target_count.saturating_add(1);
            plan.input_attachment_required |= spec.input_attachment_required;
        } else {
            plans.push(spec);
        }
    }
    Ok(plans)
}

pub(in crate::renderer::native_vulkan) fn scene_effect_target_command_plan(
    commands: &[SceneEffectTargetCommand],
    graph: &SceneRenderingDeviceGraphPlan,
) -> SceneEffectTargetCommandPlan {
    let mut plan = commands.iter().fold(
        SceneEffectTargetCommandPlan::default(),
        |mut plan, command| {
            match command.kind {
                SceneEffectTargetCommandKind::DynamicRender => {
                    if command.batch_atlas_tile.is_none() {
                        plan.dynamic_rendering_pass_count += 1;
                        plan.mesh_draw_count += command.mesh_draw_count as usize;
                        plan.fullscreen_triangle_draw_count +=
                            fullscreen_triangle_draws_in_range(graph, command);
                        plan.discard_load_count +=
                            usize::from(command.fully_overwrites_target);
                    }
                }
                SceneEffectTargetCommandKind::Copy => {
                    plan.copy_command_count +=
                        usize::from(!command.direct_scene_color_snapshot);
                }
                SceneEffectTargetCommandKind::SwapReferences => {
                    plan.swap_reference_command_count += 1;
                }
            }
            plan
        },
    );
    plan.dynamic_rendering_pass_count = plan
        .dynamic_rendering_pass_count
        .saturating_add(graph.effect_batches.len());
    plan.discard_load_count = plan
        .discard_load_count
        .saturating_add(graph.effect_batches.len());
    let mut generated_layers = Vec::<(u32, u32)>::new();
    for command in commands
        .iter()
        .filter(|command| command.batch_atlas_tile.is_some())
    {
        let key = (
            command.batch_physical_slot.unwrap_or(u32::MAX),
            command.batch_atlas_tile.unwrap_or(u32::MAX),
        );
        if generated_layers.contains(&key) {
            continue;
        }
        generated_layers.push(key);
        plan.mesh_draw_count += command.mesh_draw_count as usize;
        plan.fullscreen_triangle_draw_count += fullscreen_triangle_draws_in_range(graph, command);
    }
    plan
}

fn fullscreen_triangle_draws_in_range(
    graph: &SceneRenderingDeviceGraphPlan,
    command: &SceneEffectTargetCommand,
) -> usize {
    let start = command.mesh_draw_start as usize;
    let end = start.saturating_add(command.mesh_draw_count as usize);
    graph
        .mesh_draws
        .get(start..end)
        .unwrap_or(&[])
        .iter()
        .filter(|draw| draw.primitive == SceneRenderingDeviceDrawPrimitive::FullscreenTriangle)
        .count()
}

pub(in crate::renderer::native_vulkan) fn scene_effect_target_commands(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
) -> Vec<SceneEffectTargetCommand> {
    graph
        .pass_nodes
        .iter()
        .enumerate()
        .filter_map(|(pass_node_index, pass)| {
            let target = LogicalEffectTargetKey::from_pass_target(pass)?;
            let batch_atlas_tile =
                graph.effect_batch_atlas_tile(pass.graph_index, pass.target, pass.target_name);
            let batch_physical_slot = batch_atlas_tile
                .and_then(|_| {
                    graph.target_allocations.iter().find(|allocation| {
                        allocation.graph_index == pass.graph_index
                            && allocation.target == pass.target
                            && allocation.target_name == pass.target_name
                    })
                })
                .map(|allocation| allocation.physical_slot);
            match pass.role {
                SceneRenderPassKind::CopyTarget => {
                    let source = command_source_key(storage, pass);
                    let coverage = if source == Some(SceneEffectTargetCommandSource::SceneColor) {
                        scene_color_copy_coverage(storage, graph, pass_node_index, target)
                    } else {
                        SceneColorCopyCoverage::FullTarget
                    };
                    Some(SceneEffectTargetCommand {
                        kind: SceneEffectTargetCommandKind::Copy,
                        pass_record_index: pass.pass_record_index,
                        target,
                        source,
                        mesh_draw_start: pass.mesh_draw_start,
                        mesh_draw_count: pass.mesh_draw_count,
                        clear_before_draw: false,
                        fully_overwrites_target: false,
                        direct_scene_color_snapshot: source
                            == Some(SceneEffectTargetCommandSource::SceneColor)
                            && super::sampled_binding::target_is_direct_scene_color_snapshot(
                                graph,
                                pass.graph_index,
                                pass.target,
                                pass.target_name,
                            ),
                        scene_color_copy_coverage: coverage,
                        batch_physical_slot: None,
                        batch_atlas_tile: None,
                    })
                }
                SceneRenderPassKind::SwapTargetReferences => Some(SceneEffectTargetCommand {
                    kind: SceneEffectTargetCommandKind::SwapReferences,
                    pass_record_index: pass.pass_record_index,
                    target,
                    source: command_source_key(storage, pass),
                    mesh_draw_start: pass.mesh_draw_start,
                    mesh_draw_count: pass.mesh_draw_count,
                    clear_before_draw: false,
                    fully_overwrites_target: false,
                    direct_scene_color_snapshot: false,
                    scene_color_copy_coverage: SceneColorCopyCoverage::FullTarget,
                    batch_physical_slot: None,
                    batch_atlas_tile: None,
                }),
                _ => Some(SceneEffectTargetCommand {
                    kind: SceneEffectTargetCommandKind::DynamicRender,
                    pass_record_index: pass.pass_record_index,
                    target,
                    source: None,
                    mesh_draw_start: pass.mesh_draw_start,
                    mesh_draw_count: pass.mesh_draw_count,
                    clear_before_draw: pass.role == SceneRenderPassKind::Clear
                        || storage.document().render_passes
                            [pass.pass_record_index as usize]
                            .clear_target,
                    fully_overwrites_target: pass_fully_overwrites_target(storage, graph, pass),
                    direct_scene_color_snapshot: false,
                    scene_color_copy_coverage: SceneColorCopyCoverage::FullTarget,
                    batch_physical_slot,
                    batch_atlas_tile,
                }),
            }
        })
        .filter(|command| {
            command.kind == SceneEffectTargetCommandKind::DynamicRender || command.source.is_some()
        })
        .collect()
}

fn pass_fully_overwrites_target(
    storage: &SceneStorage,
    graph: &SceneRenderingDeviceGraphPlan,
    pass: &SceneRenderingDevicePassNode,
) -> bool {
    if pass.mesh_draw_count != 1 || pass.role == SceneRenderPassKind::Clear {
        return false;
    }
    let Some(draw) = graph.mesh_draws.get(pass.mesh_draw_start as usize) else {
        return false;
    };
    let Some(pass_record) = storage
        .document()
        .render_passes
        .get(pass.pass_record_index as usize)
    else {
        return false;
    };
    draw.primitive == SceneRenderingDeviceDrawPrimitive::FullscreenTriangle
        && matches!(
            pass_record.pipeline_blend,
            ScenePipelineBlend::Normal | ScenePipelineBlend::Disabled
        )
}

pub(in crate::renderer::native_vulkan) fn effect_batch_instance_count(
    commands: &[SceneEffectTargetCommand],
) -> usize {
    commands
        .iter()
        .filter(|command| command.batch_atlas_tile.is_some())
        .count()
}

#[cfg(test)]
mod tests;
