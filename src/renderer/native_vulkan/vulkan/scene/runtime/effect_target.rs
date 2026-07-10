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
    SceneImageTargetRecord, SceneRenderPassKind, SceneRenderTargetKind,
    SceneRenderingDeviceDrawPrimitive, SceneRenderingDeviceGraphPlan, SceneRenderingDevicePassNode,
    SceneRenderingDeviceTargetAllocation, SceneStorage, SceneStringId,
};
use crate::renderer::native_vulkan::{
    NativeVulkanVulkanaliaImage, native_vulkan_vulkanalia_create_color_attachment_sampled_image,
    native_vulkan_vulkanalia_destroy_image,
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
    pub persistent_across_frames: bool,
    pub aliased_logical_target_count: u32,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct SceneEffectTargetCommand {
    kind: SceneEffectTargetCommandKind,
    target: LogicalEffectTargetKey,
    source: Option<LogicalEffectTargetKey>,
    mesh_draw_start: u32,
    mesh_draw_count: u32,
    clear_before_draw: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SceneEffectTargetCommandKind {
    DynamicRender,
    Copy,
    SwapReferences,
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
        let spec = target_spec(storage, allocation, swapchain_format, swapchain_extent)?;
        if let Some(plan) = plans
            .iter_mut()
            .find(|plan| plan.physical_slot == allocation.physical_slot)
        {
            if (
                plan.format,
                plan.width,
                plan.height,
                plan.persistent_across_frames,
            ) != (
                spec.format,
                spec.width,
                spec.height,
                spec.persistent_across_frames,
            ) {
                return Err(format!(
                    "scene effect target physical slot {} aliases incompatible images",
                    allocation.physical_slot
                ));
            }
            plan.aliased_logical_target_count = plan.aliased_logical_target_count.saturating_add(1);
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
    commands
        .iter()
        .fold(SceneEffectTargetCommandPlan::default(), |mut plan, pass| {
            match pass.kind {
                SceneEffectTargetCommandKind::DynamicRender => {
                    plan.dynamic_rendering_pass_count += 1;
                    plan.mesh_draw_count += pass.mesh_draw_count as usize;
                    plan.fullscreen_triangle_draw_count +=
                        fullscreen_triangle_draws_in_range(graph, pass);
                }
                SceneEffectTargetCommandKind::Copy => plan.copy_command_count += 1,
                SceneEffectTargetCommandKind::SwapReferences => {
                    plan.swap_reference_command_count += 1;
                }
            }
            plan
        })
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
        .filter_map(|pass| {
            let target = LogicalEffectTargetKey::from_pass_target(pass)?;
            match pass.role {
                SceneRenderPassKind::CopyTarget => Some(SceneEffectTargetCommand {
                    kind: SceneEffectTargetCommandKind::Copy,
                    target,
                    source: command_source_key(storage, pass),
                    mesh_draw_start: pass.mesh_draw_start,
                    mesh_draw_count: pass.mesh_draw_count,
                    clear_before_draw: false,
                }),
                SceneRenderPassKind::SwapTargetReferences => Some(SceneEffectTargetCommand {
                    kind: SceneEffectTargetCommandKind::SwapReferences,
                    target,
                    source: command_source_key(storage, pass),
                    mesh_draw_start: pass.mesh_draw_start,
                    mesh_draw_count: pass.mesh_draw_count,
                    clear_before_draw: false,
                }),
                _ => Some(SceneEffectTargetCommand {
                    kind: SceneEffectTargetCommandKind::DynamicRender,
                    target,
                    source: None,
                    mesh_draw_start: pass.mesh_draw_start,
                    mesh_draw_count: pass.mesh_draw_count,
                    clear_before_draw: pass.role == SceneRenderPassKind::Clear,
                }),
            }
        })
        .filter(|command| {
            command.kind == SceneEffectTargetCommandKind::DynamicRender || command.source.is_some()
        })
        .collect()
}

pub(in crate::renderer::native_vulkan) fn create_scene_effect_target_images(
    device: &Device,
    memory_properties: &vk::PhysicalDeviceMemoryProperties,
    plans: &[SceneEffectTargetImagePlan],
) -> Result<Vec<SceneEffectTargetImageResource>, String> {
    let mut resources = Vec::with_capacity(plans.len());
    for plan in plans {
        let role = "scene-effect-target-color-attachment";
        match native_vulkan_vulkanalia_create_color_attachment_sampled_image(
            device,
            memory_properties,
            role,
            plan.format,
            plan.width,
            plan.height,
        ) {
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

pub(in crate::renderer::native_vulkan) fn effect_target_sampled_image_view_info(
    resource: &SceneEffectTargetImageResource,
) -> vk::ImageViewCreateInfo {
    vk::ImageViewCreateInfo::builder()
        .image(resource.image.image)
        .view_type(vk::ImageViewType::_2D)
        .format(resource.plan.format)
        .components(super::identity_component_mapping())
        .subresource_range(super::color_subresource_range())
        .build()
}

pub(in crate::renderer::native_vulkan) fn record_scene_effect_target_initial_layouts(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    resources: &[SceneEffectTargetImageResource],
) {
    for resource in resources {
        record_effect_target_barrier(
            device,
            command_buffer,
            resource.image.image,
            vk::ImageLayout::UNDEFINED,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::PipelineStageFlags2::TOP_OF_PIPE,
            vk::PipelineStageFlags2::ALL_TRANSFER,
            vk::AccessFlags2::empty(),
            vk::AccessFlags2::TRANSFER_WRITE,
        );
        let clear_value = vk::ClearColorValue {
            float32: [0.0, 0.0, 0.0, 0.0],
        };
        let range = super::color_subresource_range();
        unsafe {
            device.cmd_clear_color_image(
                command_buffer,
                resource.image.image,
                vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                &clear_value,
                &[range],
            );
        }
        record_effect_target_barrier(
            device,
            command_buffer,
            resource.image.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            vk::PipelineStageFlags2::ALL_TRANSFER,
            vk::PipelineStageFlags2::FRAGMENT_SHADER,
            vk::AccessFlags2::TRANSFER_WRITE,
            vk::AccessFlags2::SHADER_SAMPLED_READ,
        );
    }
}

pub(in crate::renderer::native_vulkan) fn record_scene_effect_target_graph_passes(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    graph_index: u32,
    commands: &[SceneEffectTargetCommand],
    target_allocations: &[SceneRenderingDeviceTargetAllocation],
    initial_reference_physical_slots: &[u32],
    resources: &[SceneEffectTargetImageResource],
    mut record_draws: impl FnMut(u32, u32) -> Result<(), String>,
) -> Result<(), String> {
    let mut references = logical_target_references(target_allocations);
    if references.len() != initial_reference_physical_slots.len() {
        return Err(format!(
            "scene effect target reference phase has {} slots for {} logical targets",
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
    let mut initialized_physical_slots = resources
        .iter()
        .map(|resource| resource.plan.physical_slot)
        .collect::<Vec<_>>();
    let mut initialized_logical_targets = references
        .iter()
        .filter(|reference| {
            resources.iter().any(|resource| {
                resource.plan.physical_slot == reference.physical_slot
                    && resource.plan.persistent_across_frames
            })
        })
        .map(|reference| reference.key)
        .collect::<Vec<_>>();
    for command in commands
        .iter()
        .filter(|command| command.target.graph_index == graph_index)
    {
        match command.kind {
            SceneEffectTargetCommandKind::Copy => {
                record_copy_command(device, command_buffer, *command, resources, &references)?;
                let resource = resource_for_key(resources, &references, command.target)?;
                mark_target_initialized(
                    &mut initialized_physical_slots,
                    resource.plan.physical_slot,
                );
                mark_logical_target_initialized(
                    &mut initialized_logical_targets,
                    command.target,
                );
            }
            SceneEffectTargetCommandKind::SwapReferences => {
                swap_logical_references(*command, &mut references)?;
                mark_swapped_initialized_targets(
                    *command,
                    &references,
                    &initialized_physical_slots,
                    &mut initialized_logical_targets,
                );
            }
            SceneEffectTargetCommandKind::DynamicRender => {
                let resource = resource_for_key(resources, &references, command.target)?;
                let load_op = effect_target_load_op(
                    &initialized_physical_slots,
                    resource.plan.physical_slot,
                    initialized_logical_targets.contains(&command.target),
                    command.clear_before_draw,
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
                    &mut initialized_physical_slots,
                    resource.plan.physical_slot,
                );
                mark_logical_target_initialized(
                    &mut initialized_logical_targets,
                    command.target,
                );
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
    }
    Ok(())
}

fn effect_target_load_op(
    initialized_physical_slots: &[u32],
    physical_slot: u32,
    logical_target_initialized: bool,
    clear_before_draw: bool,
) -> vk::AttachmentLoadOp {
    if clear_before_draw
        || !logical_target_initialized
        || !initialized_physical_slots.contains(&physical_slot)
    {
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
    let Some(source) = command.source else {
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

fn target_spec(
    storage: &SceneStorage,
    allocation: SceneRenderingDeviceTargetAllocation,
    swapchain_format: vk::Format,
    swapchain_extent: vk::Extent2D,
) -> Result<SceneEffectTargetImagePlan, String> {
    let image_target = storage.document().image_targets.iter().find(|target| {
        target.name == allocation.target_name && target.role == allocation.target
    });
    let format = image_target
        .and_then(|target| storage.string(target.format))
        .map(|format| target_format(format, swapchain_format))
        .transpose()?
        .unwrap_or(swapchain_format);
    let (width, height) = image_target
        .map(|target| scaled_extent(swapchain_extent, target))
        .unwrap_or((swapchain_extent.width.max(1), swapchain_extent.height.max(1)));
    Ok(SceneEffectTargetImagePlan {
        physical_slot: allocation.physical_slot,
        graph_index: allocation.graph_index,
        target: allocation.target,
        target_name: allocation.target_name,
        format,
        width,
        height,
        persistent_across_frames: matches!(
            allocation.target,
            SceneRenderTargetKind::NamedFbo | SceneRenderTargetKind::FirstClassEffectTarget
        ),
        aliased_logical_target_count: 1,
    })
}

impl LogicalEffectTargetKey {
    fn from_pass_target(pass: &SceneRenderingDevicePassNode) -> Option<Self> {
        Self::from_target(pass.graph_index, pass.target, pass.target_name)
    }

    fn from_target(
        graph_index: u32,
        target: SceneRenderTargetKind,
        name: SceneStringId,
    ) -> Option<Self> {
        effect_target_kind_is_recordable(target).then_some(Self {
            graph_index,
            target,
            name,
        })
    }
}

fn logical_target_references(
    allocations: &[SceneRenderingDeviceTargetAllocation],
) -> Vec<LogicalEffectTargetReference> {
    allocations
        .iter()
        .filter_map(|allocation| {
            LogicalEffectTargetKey::from_target(
                allocation.graph_index,
                allocation.target,
                allocation.target_name,
            )
            .map(
                |key| LogicalEffectTargetReference {
                    key,
                    physical_slot: allocation.physical_slot,
                },
            )
        })
        .collect()
}

fn command_source_key(
    storage: &SceneStorage,
    pass: &SceneRenderingDevicePassNode,
) -> Option<LogicalEffectTargetKey> {
    let start = pass.binding_start as usize;
    let end = start.saturating_add(pass.binding_count as usize);
    storage
        .document()
        .render_bindings
        .get(start..end)?
        .iter()
        .find_map(|binding| {
            LogicalEffectTargetKey::from_target(
                pass.graph_index,
                binding.target,
                binding.name,
            )
        })
}

fn record_copy_command(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    command: SceneEffectTargetCommand,
    resources: &[SceneEffectTargetImageResource],
    references: &[LogicalEffectTargetReference],
) -> Result<(), String> {
    let source_key = command
        .source
        .ok_or_else(|| "scene effect copy command has no source target binding".to_owned())?;
    let target_key = command.target;
    let source = resource_for_key(resources, references, source_key)?;
    let target = resource_for_key(resources, references, target_key)?;
    if source.plan.physical_slot == target.plan.physical_slot {
        return Ok(());
    }

    record_effect_target_barrier(
        device,
        command_buffer,
        source.image.image,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        vk::PipelineStageFlags2::FRAGMENT_SHADER,
        vk::PipelineStageFlags2::ALL_TRANSFER,
        vk::AccessFlags2::SHADER_SAMPLED_READ,
        vk::AccessFlags2::TRANSFER_READ,
    );
    record_effect_target_barrier(
        device,
        command_buffer,
        target.image.image,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        vk::PipelineStageFlags2::FRAGMENT_SHADER,
        vk::PipelineStageFlags2::ALL_TRANSFER,
        vk::AccessFlags2::SHADER_SAMPLED_READ,
        vk::AccessFlags2::TRANSFER_WRITE,
    );
    let copy_region = vk::ImageCopy::builder()
        .src_subresource(color_subresource_layers())
        .dst_subresource(color_subresource_layers())
        .extent(copy_extent(source, target))
        .build();
    unsafe {
        device.cmd_copy_image(
            command_buffer,
            source.image.image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            target.image.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[copy_region],
        );
    }
    record_effect_target_barrier(
        device,
        command_buffer,
        source.image.image,
        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        vk::PipelineStageFlags2::ALL_TRANSFER,
        vk::PipelineStageFlags2::FRAGMENT_SHADER,
        vk::AccessFlags2::TRANSFER_READ,
        vk::AccessFlags2::SHADER_SAMPLED_READ,
    );
    record_effect_target_barrier(
        device,
        command_buffer,
        target.image.image,
        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        vk::PipelineStageFlags2::ALL_TRANSFER,
        vk::PipelineStageFlags2::FRAGMENT_SHADER,
        vk::AccessFlags2::TRANSFER_WRITE,
        vk::AccessFlags2::SHADER_SAMPLED_READ,
    );
    Ok(())
}

fn swap_logical_references(
    command: SceneEffectTargetCommand,
    references: &mut [LogicalEffectTargetReference],
) -> Result<(), String> {
    let source_key = command
        .source
        .ok_or_else(|| "scene effect swap command has no source target binding".to_owned())?;
    let target_key = command.target;
    let source_index = reference_index(references, source_key).ok_or_else(|| {
        "scene effect swap command source target is not allocated".to_owned()
    })?;
    let target_index = reference_index(references, target_key).ok_or_else(|| {
        "scene effect swap command target is not allocated".to_owned()
    })?;
    references.swap(source_index, target_index);
    references[source_index].key = source_key;
    references[target_index].key = target_key;
    Ok(())
}

fn resource_for_key<'resources>(
    resources: &'resources [SceneEffectTargetImageResource],
    references: &[LogicalEffectTargetReference],
    key: LogicalEffectTargetKey,
) -> Result<&'resources SceneEffectTargetImageResource, String> {
    let reference = references
        .iter()
        .find(|reference| reference.key == key)
        .ok_or_else(|| "scene effect target is not allocated in graph".to_owned())?;
    resources
        .iter()
        .find(|resource| resource.plan.physical_slot == reference.physical_slot)
        .ok_or_else(|| {
            format!(
                "scene effect target physical slot {} has no image",
                reference.physical_slot
            )
        })
}

fn reference_index(
    references: &[LogicalEffectTargetReference],
    key: LogicalEffectTargetKey,
) -> Option<usize> {
    references
        .iter()
        .position(|reference| reference.key == key)
}

fn copy_extent(
    source: &SceneEffectTargetImageResource,
    target: &SceneEffectTargetImageResource,
) -> vk::Extent3D {
    vk::Extent3D {
        width: source.plan.width.min(target.plan.width).max(1),
        height: source.plan.height.min(target.plan.height).max(1),
        depth: 1,
    }
}

fn color_subresource_layers() -> vk::ImageSubresourceLayers {
    vk::ImageSubresourceLayers::builder()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .mip_level(0)
        .base_array_layer(0)
        .layer_count(1)
        .build()
}

fn effect_target_kind_is_recordable(target: SceneRenderTargetKind) -> bool {
    matches!(
        target,
        SceneRenderTargetKind::ImageLocalMain
            | SceneRenderTargetKind::ImageLocalSub
            | SceneRenderTargetKind::NamedFbo
            | SceneRenderTargetKind::FirstClassEffectTarget
            | SceneRenderTargetKind::Temporary
    )
}

fn target_format(format: &str, swapchain_format: vk::Format) -> Result<vk::Format, String> {
    match format {
        "r8" | "r8_unorm" => Ok(vk::Format::R8_UNORM),
        "r16f" | "r16_float" => Ok(vk::Format::R16_SFLOAT),
        "rg1616f" | "rg16f" | "rg16_float" => Ok(vk::Format::R16G16_SFLOAT),
        "rgba8" | "rgba8_unorm" | "rgba8888" | "rgba" => {
            Ok(vk::Format::R8G8B8A8_UNORM)
        }
        "rgba16f" | "rgba16_float" | "rgba16161616f" => {
            Ok(vk::Format::R16G16B16A16_SFLOAT)
        }
        "rgba_backbuffer" | "rgb_backbuffer" | "" => Ok(swapchain_format),
        _ => Err(format!(
            "scene effect target format {format:?} is not supported by the Vulkan format map"
        )),
    }
}

fn scaled_extent(extent: vk::Extent2D, target: &SceneImageTargetRecord) -> (u32, u32) {
    (
        divided_axis(extent.width, target.width_divisor_milli),
        divided_axis(extent.height, target.height_divisor_milli),
    )
}

fn divided_axis(value: u32, divisor_milli: u32) -> u32 {
    let numerator = (value.max(1) as u64).saturating_mul(1000);
    (numerator
        .div_ceil(divisor_milli.max(1) as u64)
        .min(u32::MAX as u64) as u32)
        .max(2)
}

fn record_effect_target_barrier(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
    src_stage: vk::PipelineStageFlags2,
    dst_stage: vk::PipelineStageFlags2,
    src_access: vk::AccessFlags2,
    dst_access: vk::AccessFlags2,
) {
    let barrier = vk::ImageMemoryBarrier2::builder()
        .src_stage_mask(src_stage)
        .src_access_mask(src_access)
        .dst_stage_mask(dst_stage)
        .dst_access_mask(dst_access)
        .old_layout(old_layout)
        .new_layout(new_layout)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(super::color_subresource_range())
        .build();
    let barriers = [barrier];
    let dependency = vk::DependencyInfo::builder()
        .image_memory_barriers(&barriers)
        .build();
    unsafe {
        device.cmd_pipeline_barrier2(command_buffer, &dependency);
    }
}

fn record_dynamic_rendering_pass(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    resource: &SceneEffectTargetImageResource,
    command: SceneEffectTargetCommand,
    load_op: vk::AttachmentLoadOp,
    record_draws: &mut impl FnMut(u32, u32) -> Result<(), String>,
) -> Result<(), String> {
    let clear_value = vk::ClearValue {
        color: vk::ClearColorValue {
            float32: [0.0, 0.0, 0.0, 0.0],
        },
    };
    let attachment = vk::RenderingAttachmentInfo::builder()
        .image_view(resource.image.view)
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(load_op)
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(clear_value)
        .build();
    let attachments = [attachment];
    let render_area = vk::Rect2D::builder()
        .offset(vk::Offset2D { x: 0, y: 0 })
        .extent(vk::Extent2D {
            width: resource.plan.width,
            height: resource.plan.height,
        })
        .build();
    let rendering = vk::RenderingInfo::builder()
        .render_area(render_area)
        .layer_count(1)
        .color_attachments(&attachments)
        .build();
    unsafe {
        device.cmd_begin_rendering(command_buffer, &rendering);
    }
    super::draw_recording::record_scene_draw_extent(
        device,
        command_buffer,
        vk::Extent2D {
            width: resource.plan.width,
            height: resource.plan.height,
        },
    );
    let draw_result = if command.mesh_draw_count == 0 {
        Ok(())
    } else {
        record_draws(command.mesh_draw_start, command.mesh_draw_count)
    };
    unsafe {
        device.cmd_end_rendering(command_buffer);
    }
    draw_result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{
        SceneBinaryDocument, SceneRenderingDeviceGraphPlan, SceneRenderingDeviceTargetAllocation,
    };

    #[test]
    fn effect_target_image_plan_scales_and_aliases_physical_slots() {
        let storage = SceneStorage::from_document(SceneBinaryDocument {
            strings: vec!["rt_a".to_owned(), "rgba8".to_owned(), "fbo_b".to_owned()],
            image_targets: vec![
                SceneImageTargetRecord {
                    name: SceneStringId(0),
                    role: SceneRenderTargetKind::FirstClassEffectTarget,
                    format: SceneStringId(1),
                    width_divisor_milli: 2_000,
                    height_divisor_milli: 4_000,
                },
                SceneImageTargetRecord {
                    name: SceneStringId(2),
                    role: SceneRenderTargetKind::NamedFbo,
                    format: SceneStringId(1),
                    width_divisor_milli: 2_000,
                    height_divisor_milli: 4_000,
                },
            ],
            ..SceneBinaryDocument::default()
        })
        .expect("storage");
        let graph = graph_with_allocations(vec![
            allocation(2, SceneRenderTargetKind::FirstClassEffectTarget, SceneStringId(0)),
            allocation(2, SceneRenderTargetKind::NamedFbo, SceneStringId(2)),
        ]);

        let plans = scene_effect_target_image_plan(
            &storage,
            &graph,
            vk::Format::B8G8R8A8_UNORM,
            vk::Extent2D {
                width: 1920,
                height: 1080,
            },
        )
        .expect("effect target plan");

        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].physical_slot, 2);
        assert_eq!(plans[0].format, vk::Format::R8G8B8A8_UNORM);
        assert_eq!(plans[0].width, 960);
        assert_eq!(plans[0].height, 270);
        assert!(plans[0].persistent_across_frames);
        assert_eq!(plans[0].aliased_logical_target_count, 2);
    }

    #[test]
    fn effect_target_image_plan_uses_backbuffer_format_for_missing_target_records() {
        let storage =
            SceneStorage::from_document(SceneBinaryDocument::default()).expect("storage");
        let graph = graph_with_allocations(vec![allocation(
            0,
            SceneRenderTargetKind::NamedFbo,
            SceneStringId(5),
        )]);

        let plans = scene_effect_target_image_plan(
            &storage,
            &graph,
            vk::Format::B8G8R8A8_UNORM,
            vk::Extent2D {
                width: 1280,
                height: 720,
            },
        )
        .expect("effect target plan");

        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].format, vk::Format::B8G8R8A8_UNORM);
        assert_eq!(plans[0].width, 1280);
        assert_eq!(plans[0].height, 720);
    }

    #[test]
    fn effect_target_image_plan_rejects_incompatible_manual_aliases() {
        let storage = SceneStorage::from_document(SceneBinaryDocument {
            strings: vec![
                "fbo_a".to_owned(),
                "rgba8".to_owned(),
                "fbo_b".to_owned(),
                "rgba16f".to_owned(),
            ],
            image_targets: vec![
                SceneImageTargetRecord {
                    name: SceneStringId(0),
                    role: SceneRenderTargetKind::NamedFbo,
                    format: SceneStringId(1),
                    width_divisor_milli: 1_000,
                    height_divisor_milli: 1_000,
                },
                SceneImageTargetRecord {
                    name: SceneStringId(2),
                    role: SceneRenderTargetKind::NamedFbo,
                    format: SceneStringId(3),
                    width_divisor_milli: 2_000,
                    height_divisor_milli: 2_000,
                },
            ],
            ..SceneBinaryDocument::default()
        })
        .expect("storage");
        let graph = graph_with_allocations(vec![
            allocation(0, SceneRenderTargetKind::NamedFbo, SceneStringId(0)),
            allocation(0, SceneRenderTargetKind::NamedFbo, SceneStringId(2)),
        ]);

        let error = scene_effect_target_image_plan(
            &storage,
            &graph,
            vk::Format::B8G8R8A8_UNORM,
            vk::Extent2D {
                width: 1920,
                height: 1080,
            },
        )
        .expect_err("incompatible alias must fail");

        assert!(error.contains("aliases incompatible images"));
        assert_eq!(divided_axis(1920, 4_000), 480);
        assert_eq!(divided_axis(1080, 4_000), 270);
        assert_eq!(
            target_format("r16f", vk::Format::B8G8R8A8_UNORM).expect("r16f"),
            vk::Format::R16_SFLOAT
        );
        assert_eq!(
            target_format("rg1616f", vk::Format::B8G8R8A8_UNORM).expect("rg1616f"),
            vk::Format::R16G16_SFLOAT
        );
        assert_eq!(
            target_format("rgba8888", vk::Format::B8G8R8A8_UNORM).expect("rgba8888"),
            vk::Format::R8G8B8A8_UNORM
        );
    }

    #[test]
    fn effect_target_commands_track_copy_swap_and_dynamic_passes() {
        let storage = SceneStorage::from_document(SceneBinaryDocument {
            strings: vec!["fbo_a".to_owned(), "fbo_b".to_owned()],
            render_bindings: vec![
                named_fbo_binding(SceneStringId(0)),
                named_fbo_binding(SceneStringId(0)),
            ],
            ..SceneBinaryDocument::default()
        })
        .expect("storage");
        let mut effect_pass = pass_node(3, SceneRenderPassKind::EffectMaterial, SceneStringId(1), 2);
        effect_pass.mesh_draw_start = 4;
        effect_pass.mesh_draw_count = 2;
        let graph = SceneRenderingDeviceGraphPlan {
            pass_nodes: vec![
                pass_node(1, SceneRenderPassKind::CopyTarget, SceneStringId(1), 0),
                pass_node(
                    2,
                    SceneRenderPassKind::SwapTargetReferences,
                    SceneStringId(1),
                    1,
                ),
                effect_pass,
            ],
            target_allocations: vec![
                allocation(0, SceneRenderTargetKind::NamedFbo, SceneStringId(0)),
                allocation(1, SceneRenderTargetKind::NamedFbo, SceneStringId(1)),
            ],
            graph_physical_target_count: 2,
            ..empty_graph_plan()
        };

        let commands = scene_effect_target_commands(&storage, &graph);
        let plan = scene_effect_target_command_plan(&commands, &graph);
        let mut references = logical_target_references(&graph.target_allocations);

        assert_eq!(commands.len(), 3);
        assert_eq!(plan.copy_command_count, 1);
        assert_eq!(plan.swap_reference_command_count, 1);
        assert_eq!(plan.dynamic_rendering_pass_count, 1);
        assert_eq!(plan.mesh_draw_count, 2);
        assert_eq!(commands[2].mesh_draw_start, 4);
        assert_eq!(commands[2].mesh_draw_count, 2);

        swap_logical_references(commands[1], &mut references).expect("swap refs");
        assert_eq!(
            references
                .iter()
                .find(|reference| reference.key.name == SceneStringId(0))
                .expect("fbo_a")
                .physical_slot,
            1
        );
        assert_eq!(
            references
                .iter()
                .find(|reference| reference.key.name == SceneStringId(1))
                .expect("fbo_b")
                .physical_slot,
            0
        );
    }

    #[test]
    fn repeated_effect_target_passes_load_after_the_initial_clear() {
        let mut initialized = Vec::new();

        assert_eq!(
            effect_target_load_op(&initialized, 4, false, false),
            vk::AttachmentLoadOp::CLEAR
        );
        mark_target_initialized(&mut initialized, 4);
        assert_eq!(
            effect_target_load_op(&initialized, 4, true, false),
            vk::AttachmentLoadOp::LOAD
        );
        assert_eq!(
            effect_target_load_op(&initialized, 4, true, true),
            vk::AttachmentLoadOp::CLEAR
        );
    }

    fn graph_with_allocations(
        target_allocations: Vec<SceneRenderingDeviceTargetAllocation>,
    ) -> SceneRenderingDeviceGraphPlan {
        SceneRenderingDeviceGraphPlan {
            target_allocations,
            graph_physical_target_count: 1,
            ..empty_graph_plan()
        }
    }

    fn pass_node(
        pass_id: u32,
        role: SceneRenderPassKind,
        target_name: SceneStringId,
        binding_start: u32,
    ) -> SceneRenderingDevicePassNode {
        SceneRenderingDevicePassNode {
            graph_index: 0,
            pass_record_index: pass_id,
            pass_id,
            role,
            target: SceneRenderTargetKind::NamedFbo,
            target_name,
            binding_start,
            binding_count: u32::from(binding_start < 2),
            mesh_draw_start: 0,
            mesh_draw_count: 0,
        }
    }

    fn allocation(
        physical_slot: u32,
        target: SceneRenderTargetKind,
        target_name: SceneStringId,
    ) -> SceneRenderingDeviceTargetAllocation {
        SceneRenderingDeviceTargetAllocation {
            graph_index: 0,
            target,
            target_name,
            first_write_pass_id: 1,
            last_use_pass_id: 2,
            physical_slot,
        }
    }

    fn named_fbo_binding(name: SceneStringId) -> crate::engine::scene::SceneRenderBindingRecord {
        crate::engine::scene::SceneRenderBindingRecord {
            kind: crate::engine::scene::SceneRenderBindingKind::NamedFboBind,
            slot: 0,
            target: SceneRenderTargetKind::NamedFbo,
            name,
        }
    }

    fn empty_graph_plan() -> SceneRenderingDeviceGraphPlan {
        SceneRenderingDeviceGraphPlan {
            pass_nodes: Vec::new(),
            target_allocations: Vec::new(),
            sampled_bindings: Vec::new(),
            material_sampled_bindings: Vec::new(),
            mesh_draws: Vec::new(),
            puppet_bone_palettes: Vec::new(),
            puppet_bone_matrices: Vec::new(),
            resolved_object_count: 0,
            resolved_visible_object_count: 0,
            resolved_attachment_link_count: 0,
            resolved_visible_effect_instance_count: 0,
            resolved_visible_effect_pass_count: 0,
            resolved_visible_effect_fbo_count: 0,
            descriptor_heap_required: true,
            descriptor_heap_resource_count: 0,
            descriptor_heap_sampled_image_count: 0,
            descriptor_heap_uniform_buffer_count: 0,
            descriptor_heap_storage_buffer_count: 0,
            descriptor_heap_sampler_count: 0,
            graph_physical_target_count: 0,
            graph_aliased_target_count: 0,
            fifo_latest_ready_present_required: true,
        }
    }
}
