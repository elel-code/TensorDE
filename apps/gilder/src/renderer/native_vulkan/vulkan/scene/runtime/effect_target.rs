//! Vulkan scene effect target images for dynamic-rendering graph passes.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/exe/composelayer-and-effecttarget.md`
//! - `references/godot/servers/rendering/rendering_device_graph.*`

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use crate::engine::scene::{
    SceneImageTargetRecord, ScenePipelineBlend, SceneRenderPassKind, SceneRenderTargetKind,
    SceneRenderingDeviceDrawPrimitive, SceneRenderingDeviceGraphPlan, SceneRenderingDeviceImageAccess,
    SceneRenderingDevicePassNode, SceneRenderingDeviceTargetAllocation, SceneStorage, SceneStringId,
};
use crate::renderer::native_vulkan::{
    NativeVulkanVulkanaliaImage,
    native_vulkan_vulkanalia_create_color_attachment_sampled_image_with_usage,
    native_vulkan_vulkanalia_destroy_image,
};

mod image_commands;
mod execution_state;
mod local_read_scope;
mod local_read_usage;
mod scene_color_copy;

pub(in crate::renderer::native_vulkan) use execution_state::SceneEffectTargetExecutionState;
use image_commands::*;
use local_read_scope::*;
use scene_color_copy::{
    SceneColorCopyCoverage, scene_color_copy_coverage, scene_color_copy_region,
};
pub(in crate::renderer::native_vulkan) use scene_color_copy::{
    graph_copies_scene_color, graph_requires_effect_target_execution,
    graph_uses_direct_scene_color_snapshot,
};
use super::draw_recording::SceneGpuDrawCommand;
use super::local_read::{SceneLocalReadDeviceLimits, SceneLocalReadScopePlan};
pub(in crate::renderer::native_vulkan) use local_read_usage::{
    apply_scene_effect_target_input_attachment_usage,
    apply_scene_effect_target_local_read_candidate_usage,
    apply_scene_effect_target_local_read_scope_usage,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct SceneEffectTargetImagePlan {
    pub physical_slot: u32,
    pub graph_index: u32,
    pub target: SceneRenderTargetKind,
    pub target_name: SceneStringId,
    pub format: vk::Format,
    pub width: u32,
    pub height: u32,
    pub batch_field_count: u32,
    pub batch_atlas_columns: u32,
    pub batch_atlas_rows: u32,
    pub persistent_across_frames: bool,
    pub aliased_logical_target_count: u32,
    pub input_attachment_required: bool,
}

pub(in crate::renderer::native_vulkan) struct SceneEffectTargetImageResource {
    pub plan: SceneEffectTargetImagePlan,
    pub image: NativeVulkanVulkanaliaImage,
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
    swapchain_format: vk::Format,
    swapchain_extent: vk::Extent2D,
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
            let field_extent_divisor =
                u64::from(graph.effect_batch_field_extent_divisor(allocation.physical_slot));
            spec.width = (u64::from(spec.width)
                .div_ceil(field_extent_divisor)
                .saturating_mul(u64::from(spec.batch_atlas_columns))
                .min(u64::from(u32::MAX)) as u32)
                .max(spec.batch_atlas_columns);
            spec.height = (u64::from(spec.height)
                .div_ceil(field_extent_divisor)
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
            plan.width = plan.width.max(spec.width);
            plan.height = plan.height.max(spec.height);
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
    let mut plan =
        commands
            .iter()
            .fold(SceneEffectTargetCommandPlan::default(), |mut plan, pass| {
                match pass.kind {
                    SceneEffectTargetCommandKind::DynamicRender => {
                        if pass.batch_atlas_tile.is_none() {
                            plan.dynamic_rendering_pass_count += 1;
                            plan.mesh_draw_count += pass.mesh_draw_count as usize;
                            plan.fullscreen_triangle_draw_count +=
                                fullscreen_triangle_draws_in_range(graph, pass);
                            plan.discard_load_count += usize::from(pass.fully_overwrites_target);
                        }
                    }
                    SceneEffectTargetCommandKind::Copy => {
                        plan.copy_command_count += usize::from(!pass.direct_scene_color_snapshot);
                    }
                    SceneEffectTargetCommandKind::SwapReferences => {
                        plan.swap_reference_command_count += 1;
                    }
                }
                plan
            });
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
                    let scene_color_copy_coverage = if source
                        == Some(SceneEffectTargetCommandSource::SceneColor)
                    {
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
                        scene_color_copy_coverage,
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
                        || storage
                            .document()
                            .render_passes
                            .get(pass.pass_record_index as usize)
                            .expect(
                                "RenderingDevice pass nodes reference validated scene pass records",
                            )
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

pub(in crate::renderer::native_vulkan) fn create_scene_effect_target_images(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    plans: &[SceneEffectTargetImagePlan],
) -> Result<Vec<SceneEffectTargetImageResource>, String> {
    let mut resources = Vec::with_capacity(plans.len());
    for plan in plans {
        let role = "scene-effect-target-color-attachment";
        let image = native_vulkan_vulkanalia_create_color_attachment_sampled_image_with_usage(
            device,
            memory_properties,
            role,
            plan.format,
            plan.width,
            plan.height,
            plan.input_attachment_required,
        );
        match image {
            Ok(image) => resources.push(SceneEffectTargetImageResource {
                plan: plan.clone(),
                image,
            }),
            Err(err) => {
                destroy_scene_effect_target_images(device, resources);
                return Err(err);
            }
        }
    }
    Ok(resources)
}

pub(in crate::renderer::native_vulkan) fn destroy_scene_effect_target_images(
    device: &Device,
    resources: Vec<SceneEffectTargetImageResource>,
) {
    for resource in resources {
        native_vulkan_vulkanalia_destroy_image(device, resource.image);
    }
}

pub(in crate::renderer::native_vulkan) fn effect_target_memory_bytes(
    resources: &[SceneEffectTargetImageResource],
) -> u64 {
    resources
        .iter()
        .map(|resource| resource.image.snapshot.memory_size)
        .sum()
}

pub(in crate::renderer::native_vulkan) fn effect_target_image_view_info(
    resource: &SceneEffectTargetImageResource,
    _batch_atlas_tile: u32,
) -> vk::ImageViewCreateInfo {
    vk::ImageViewCreateInfo::builder()
        .image(resource.image.image)
        .view_type(vk::ImageViewType::_2D)
        .format(resource.plan.format)
        .components(super::identity_component_mapping())
        .subresource_range(effect_target_subresource_range(0, 1))
        .build()
}

pub(in crate::renderer::native_vulkan) fn record_scene_effect_target_initial_layouts(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    resources: &[SceneEffectTargetImageResource],
) {
    for resource in resources {
        record_effect_target_barrier_layers(
            device,
            command_buffer,
            resource.image.image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::PipelineStageFlags2::TOP_OF_PIPE,
            vk::PipelineStageFlags2::ALL_TRANSFER,
            vk::AccessFlags2::empty(),
            vk::AccessFlags2::TRANSFER_WRITE,
            1,
        );
        let clear_value = vk::ClearColorValue {
            float32: [0.0, 0.0, 0.0, 0.0],
        };
        let range = effect_target_subresource_range(0, 1);
        unsafe {
            device.cmd_clear_color_image(
                command_buffer,
                resource.image.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &clear_value,
                &[range],
            );
        }
        record_effect_target_barrier_layers(
            device,
            command_buffer,
            resource.image.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::PipelineStageFlags2::ALL_TRANSFER,
            vk::PipelineStageFlags2::FRAGMENT_SHADER,
            vk::AccessFlags2::TRANSFER_WRITE,
            vk::AccessFlags2::SHADER_SAMPLED_READ,
            1,
        );
    }
}

pub(in crate::renderer::native_vulkan) fn record_scene_effect_batches(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    commands: &[SceneEffectTargetCommand],
    resources: &[SceneEffectTargetImageResource],
    mut record_draws: impl FnMut(u32, u32, vk::Extent2D) -> Result<(), String>,
) -> Result<(), String> {
    for resource in resources
        .iter()
        .filter(|resource| resource.plan.batch_field_count > 1)
    {
        let mut generated_layers = Vec::new();
        let batch_commands = commands
            .iter()
            .filter(|command| {
                if command.batch_physical_slot != Some(resource.plan.physical_slot) {
                    return false;
                }
                let Some(layer) = command.batch_atlas_tile else {
                    return false;
                };
                if generated_layers.contains(&layer) {
                    return false;
                }
                generated_layers.push(layer);
                true
            })
            .collect::<Vec<_>>();
        if batch_commands.is_empty() {
            continue;
        }
        if batch_commands.len() > resource.plan.batch_field_count as usize {
            return Err(format!(
                "scene effect batch has {} instances for {} image layers",
                batch_commands.len(),
                resource.plan.batch_field_count
            ));
        }
        record_effect_target_barrier_layers(
            device,
            command_buffer,
            resource.image.image,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::PipelineStageFlags2::FRAGMENT_SHADER,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::SHADER_SAMPLED_READ,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            1,
        );
        let clear_value = vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.0, 0.0, 0.0, 0.0],
            },
        };
        let attachment = vk::RenderingAttachmentInfo::builder()
            .image_view(resource.image.view)
            .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .load_op(vk::AttachmentLoadOp::DONT_CARE)
            .store_op(vk::AttachmentStoreOp::STORE)
            .clear_value(clear_value)
            .build();
        let extent = vk::Extent2D {
            width: resource.plan.width,
            height: resource.plan.height,
        };
        let attachments = [attachment];
        let rendering = vk::RenderingInfo::builder()
            .render_area(
                vk::Rect2D::builder()
                    .offset(vk::Offset2D { x: 0, y: 0 })
                    .extent(extent)
                    .build(),
            )
            .layer_count(1)
            .color_attachments(&attachments)
            .build();
        unsafe {
            device.cmd_begin_rendering(command_buffer, &rendering);
        }
        super::draw_recording::record_scene_draw_extent(device, command_buffer, extent);
        let mut draw_result = Ok(());
        for command in batch_commands {
            if let Err(err) = record_draws(command.mesh_draw_start, command.mesh_draw_count, extent)
            {
                draw_result = Err(err);
                break;
            }
        }
        unsafe {
            device.cmd_end_rendering(command_buffer);
        }
        draw_result?;
        record_effect_target_barrier_layers(
            device,
            command_buffer,
            resource.image.image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags2::FRAGMENT_SHADER,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            vk::AccessFlags2::SHADER_SAMPLED_READ,
            1,
        );
    }
    Ok(())
}

pub(in crate::renderer::native_vulkan) fn record_scene_effect_target_graph_passes(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    scene_color_image: vk::Image,
    scene_color_extent: vk::Extent2D,
    graph_index: u32,
    command_position_offset: usize,
    commands: &[SceneEffectTargetCommand],
    target_allocations: &[SceneRenderingDeviceTargetAllocation],
    initial_reference_physical_slots: &[u32],
    resources: &[SceneEffectTargetImageResource],
    local_read_scopes: &[SceneLocalReadScopePlan],
    local_read_limits: SceneLocalReadDeviceLimits,
    draw_commands: &[SceneGpuDrawCommand],
    mut record_draws: impl FnMut(u32, u32, vk::Extent2D) -> Result<(), String>,
    mut record_command_timing: impl FnMut(usize, bool),
) -> Result<(), String> {
    let mut state = SceneEffectTargetExecutionState::new(
        target_allocations,
        initial_reference_physical_slots,
        resources,
    )?;
    record_scene_effect_target_graph_passes_with_state(
        device,
        command_buffer,
        scene_color_image,
        scene_color_extent,
        graph_index,
        command_position_offset,
        commands,
        resources,
        local_read_scopes,
        local_read_limits,
        draw_commands,
        &mut state,
        &mut record_draws,
        &mut record_command_timing,
    )
}

/// Records a command slice while retaining caller-owned target state across interleaved passes.
pub(in crate::renderer::native_vulkan) fn record_scene_effect_target_graph_passes_with_state(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    scene_color_image: vk::Image,
    scene_color_extent: vk::Extent2D,
    graph_index: u32,
    command_position_offset: usize,
    commands: &[SceneEffectTargetCommand],
    resources: &[SceneEffectTargetImageResource],
    local_read_scopes: &[SceneLocalReadScopePlan],
    local_read_limits: SceneLocalReadDeviceLimits,
    draw_commands: &[SceneGpuDrawCommand],
    state: &mut SceneEffectTargetExecutionState,
    mut record_draws: impl FnMut(u32, u32, vk::Extent2D) -> Result<(), String>,
    mut record_command_timing: impl FnMut(usize, bool),
) -> Result<(), String> {
    let references = &mut state.references;
    let initialized_physical_slots = &mut state.initialized_physical_slots;
    let initialized_logical_targets = &mut state.initialized_logical_targets;
    let mut next_command_position = 0usize;
    while let Some((relative_position, command)) = commands
        .iter()
        .enumerate()
        .skip(next_command_position)
        .find(|command| {
            command.1.target.graph_index == graph_index && command.1.batch_atlas_tile.is_none()
        })
    {
        next_command_position = relative_position + 1;
        let command_position = command_position_offset.saturating_add(relative_position);
        if let Some(scope) = local_read_scopes
            .iter()
            .filter(|scope| scope.graph_index() == graph_index)
            .find(|scope| local_read_scope_matches_command(scope, command, true))
        {
            let (consumer_relative_position, consumer) = commands
                .iter()
                .enumerate()
                .skip(next_command_position)
                .find(|consumer| {
                    consumer.1.target.graph_index == graph_index
                        && consumer.1.batch_atlas_tile.is_none()
                })
                .ok_or_else(|| {
                    format!(
                        "scene graph {graph_index} local-read producer pass record {} has no adjacent consumer command",
                        command.pass_record_index
                    )
                })?;
            next_command_position = consumer_relative_position + 1;
            let consumer_position =
                command_position_offset.saturating_add(consumer_relative_position);
            if !local_read_scope_matches_command(scope, consumer, false) {
                return Err(format!(
                    "scene graph {graph_index} local-read producer pass record {} is followed by pass record {}, not its planned consumer {}",
                    command.pass_record_index,
                    consumer.pass_record_index,
                    scope.consumer_pass_record_index()
                ));
            }
            let source = resource_for_key(resources, &references, command.target)?;
            let destination = resource_for_key(resources, &references, consumer.target)?;
            let source_load_op = effect_target_load_op(
                &initialized_physical_slots,
                source.plan.physical_slot,
                initialized_logical_targets.contains(&command.target),
                command.clear_before_draw,
                command.fully_overwrites_target,
            );
            let destination_load_op = effect_target_load_op(
                &initialized_physical_slots,
                destination.plan.physical_slot,
                initialized_logical_targets.contains(&consumer.target),
                consumer.clear_before_draw,
                consumer.fully_overwrites_target,
            );
            record_scene_local_read_scope(
                device,
                command_buffer,
                source,
                destination,
                *command,
                *consumer,
                source_load_op,
                destination_load_op,
                scope,
                local_read_limits,
                command_position,
                consumer_position,
                &mut record_draws,
                &mut record_command_timing,
            )?;
            for (key, resource) in [(command.target, source), (consumer.target, destination)] {
                mark_target_initialized(
                    initialized_physical_slots,
                    resource.plan.physical_slot,
                );
                mark_logical_target_initialized(initialized_logical_targets, key);
            }
            continue;
        }
        if local_read_scopes.iter().any(|scope| {
            scope.graph_index() == graph_index
                && local_read_scope_matches_command(scope, command, false)
        }) {
            return Err(format!(
                "scene graph {graph_index} local-read consumer pass record {} was not recorded with its producer",
                command.pass_record_index
            ));
        }
        record_command_timing(command_position, true);
        match command.kind {
            SceneEffectTargetCommandKind::Copy => {
                if !command.direct_scene_color_snapshot {
                    record_copy_command(
                        device,
                        command_buffer,
                        scene_color_image,
                        scene_color_extent,
                        *command,
                        resources,
                        &references,
                        draw_commands,
                    )?;
                    let resource = resource_for_key(resources, &references, command.target)?;
                    mark_target_initialized(
                        initialized_physical_slots,
                        resource.plan.physical_slot,
                    );
                }
                mark_logical_target_initialized(initialized_logical_targets, command.target);
            }
            SceneEffectTargetCommandKind::SwapReferences => {
                swap_logical_references(*command, references)?;
                mark_swapped_initialized_targets(
                    *command,
                    &references,
                    &initialized_physical_slots,
                    initialized_logical_targets,
                );
            }
            SceneEffectTargetCommandKind::DynamicRender => {
                let resource = resource_for_key(resources, &references, command.target)?;
                let load_op = effect_target_load_op(
                    &initialized_physical_slots,
                    resource.plan.physical_slot,
                    initialized_logical_targets.contains(&command.target),
                    command.clear_before_draw,
                    command.fully_overwrites_target,
                );
                record_effect_target_barrier(
                    device,
                    command_buffer,
                    resource.image.image,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    vk::PipelineStageFlags2::FRAGMENT_SHADER,
                    vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    vk::AccessFlags2::SHADER_SAMPLED_READ,
                    vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                );
                record_dynamic_rendering_pass(
                    device,
                    command_buffer,
                    resource,
                    *command,
                    load_op,
                    &mut record_draws,
                )?;
                mark_target_initialized(
                    initialized_physical_slots,
                    resource.plan.physical_slot,
                );
                mark_logical_target_initialized(initialized_logical_targets, command.target);
                record_effect_target_barrier(
                    device,
                    command_buffer,
                    resource.image.image,
                    vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
                    vk::PipelineStageFlags2::FRAGMENT_SHADER,
                    vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
                    vk::AccessFlags2::SHADER_SAMPLED_READ,
                );
            }
        }
        record_command_timing(command_position, false);
    }
    Ok(())
}

pub(in crate::renderer::native_vulkan) fn record_scene_effect_target_pass_with_state(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    scene_color_image: vk::Image,
    scene_color_extent: vk::Extent2D,
    pass: &SceneRenderingDevicePassNode,
    commands: &[SceneEffectTargetCommand],
    resources: &[SceneEffectTargetImageResource],
    local_read_scopes: &[SceneLocalReadScopePlan],
    local_read_limits: SceneLocalReadDeviceLimits,
    draw_commands: &[SceneGpuDrawCommand],
    state: &mut SceneEffectTargetExecutionState,
    record_draws: impl FnMut(u32, u32, vk::Extent2D) -> Result<(), String>,
    mut record_command_timing: impl FnMut(usize, bool),
) -> Result<(), String> {
    let (command_position, command) = scene_effect_target_command_for_pass(commands, pass)?;
    record_command_timing(command_position, true);
    let result = record_scene_effect_target_graph_passes_with_state(
        device,
        command_buffer,
        scene_color_image,
        scene_color_extent,
        pass.graph_index,
        command_position,
        std::slice::from_ref(&command),
        resources,
        local_read_scopes,
        local_read_limits,
        draw_commands,
        state,
        record_draws,
        |_, _| {},
    );
    record_command_timing(command_position, false);
    result
}

pub(in crate::renderer::native_vulkan) fn scene_effect_target_command_for_pass(
    commands: &[SceneEffectTargetCommand],
    pass: &SceneRenderingDevicePassNode,
) -> Result<(usize, SceneEffectTargetCommand), String> {
    commands
        .iter()
        .enumerate()
        .find(|command| {
            command.1.target.graph_index == pass.graph_index
                && command.1.pass_record_index == pass.pass_record_index
                && command.1.target.target == pass.target
                && command.1.target.name == pass.target_name
                && command.1.mesh_draw_start == pass.mesh_draw_start
                && command.1.mesh_draw_count == pass.mesh_draw_count
        })
        .map(|(position, command)| (position, *command))
        .ok_or_else(|| {
            format!(
                "scene graph {} pass {} has no matching effect-target command",
                pass.graph_index, pass.pass_id
            )
        })
}

pub(in crate::renderer::native_vulkan) fn scene_effect_target_command_is_dynamic(
    command: SceneEffectTargetCommand,
) -> bool {
    matches!(command.kind, SceneEffectTargetCommandKind::DynamicRender)
}

fn effect_target_load_op(
    initialized_physical_slots: &[u32],
    physical_slot: u32,
    logical_target_initialized: bool,
    clear_before_draw: bool,
    fully_overwrites_target: bool,
) -> vk::AttachmentLoadOp {
    if clear_before_draw {
        vk::AttachmentLoadOp::CLEAR
    } else if fully_overwrites_target {
        vk::AttachmentLoadOp::DONT_CARE
    } else if !logical_target_initialized || !initialized_physical_slots.contains(&physical_slot) {
        vk::AttachmentLoadOp::CLEAR
    } else {
        vk::AttachmentLoadOp::LOAD
    }
}

fn mark_logical_target_initialized(
    initialized_logical_targets: &mut Vec<LogicalEffectTargetKey>,
    key: LogicalEffectTargetKey,
) {
    if !initialized_logical_targets.contains(&key) {
        initialized_logical_targets.push(key);
    }
}

fn mark_swapped_initialized_targets(
    command: SceneEffectTargetCommand,
    references: &[LogicalEffectTargetReference],
    initialized_physical_slots: &[u32],
    initialized_logical_targets: &mut Vec<LogicalEffectTargetKey>,
) {
    let Some(SceneEffectTargetCommandSource::LogicalTarget(source)) = command.source else {
        return;
    };
    for key in [source, command.target] {
        let Some(index) = reference_index(references, key) else {
            continue;
        };
        if initialized_physical_slots.contains(&references[index].physical_slot) {
            mark_logical_target_initialized(initialized_logical_targets, key);
        }
    }
}

fn mark_target_initialized(initialized_physical_slots: &mut Vec<u32>, physical_slot: u32) {
    if !initialized_physical_slots.contains(&physical_slot) {
        initialized_physical_slots.push(physical_slot);
    }
}

#[cfg(test)]
mod tests;
