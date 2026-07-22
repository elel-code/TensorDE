//! Interleaved effect-target and scene-color graph execution.
//!
//! Each object graph composites to scene color before its aliased image-local
//! targets are reused by the next graph.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `references/godot/servers/rendering/rendering_device_graph.*`

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, HasBuilder};

use crate::engine::scene::{
    SceneRenderTargetKind, SceneRenderingDeviceGraphPlan, SceneRenderingDevicePassNode,
};

use super::draw_recording::{
    SceneGpuDrawCommand, SceneGpuDrawRange, record_scene_draw_extent, record_scene_mesh_draw_ranges,
};
use super::gpu_timing::SceneGpuTiming;
use super::{NativeVulkanClearColor, SceneGpuResources, color_subresource_range, effect_target};

pub(super) fn scene_graph_execution_order(
    graph: &SceneRenderingDeviceGraphPlan,
) -> Vec<u32> {
    let mut order = Vec::new();
    for pass in graph
        .pass_nodes
        .iter()
        .filter(|pass| pass.mesh_draw_count != 0)
    {
        if order.last().copied() != Some(pass.graph_index) {
            order.push(pass.graph_index);
        }
    }
    order
}

pub(super) fn record_scene_graphs_to_swapchain(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    swapchain_image: vk::Image,
    swapchain_view: vk::ImageView,
    old_layout: vk::ImageLayout,
    extent: vk::Extent2D,
    clear_color: NativeVulkanClearColor,
    scene: &SceneGpuResources,
    reference_phase: usize,
    gpu_timing: Option<&SceneGpuTiming>,
) -> Result<(), String> {
    super::particle_compute_dispatch::record_particle_compute_dispatch(
        device,
        command_buffer,
        scene,
    )?;
    transition_swapchain_to_attachment(device, command_buffer, swapchain_image, old_layout);
    transition_scene_color_msaa_to_attachment(device, command_buffer, scene);
    let reference_slots = &scene
        .sampled_binding_cycle
        .get(reference_phase)
        .ok_or_else(|| format!("scene sampled binding phase {reference_phase} is missing"))?
        .initial_reference_physical_slots;
    let mut record_batch_draws = |draw_start, draw_count, target_extent| {
        record_scene_mesh_draw_ranges(
            device,
            command_buffer,
            scene,
            &[SceneGpuDrawRange {
                start: draw_start,
                count: draw_count,
            }],
            target_extent,
        )
    };
    if let Some(timing) = gpu_timing {
        timing.record_effect_batch_start(device, command_buffer);
    }
    effect_target::record_scene_effect_batches(
        device,
        command_buffer,
        &scene.effect_target_commands,
        &scene.effect_targets,
        &mut record_batch_draws,
    )?;
    if let Some(timing) = gpu_timing {
        timing.record_effect_batch_finish(device, command_buffer);
    }
    let mut scene_color_initialized = false;
    let mut scene_color_rendering_active = false;
    let mut scene_color_resolve_dirty = false;

    for (graph_position, graph_index) in scene.graph_execution_order.iter().enumerate() {
        if let Some(timing) = gpu_timing {
            timing.record_graph_start(device, command_buffer, graph_position);
            timing.record_graph_effect_target_start(device, command_buffer, graph_position);
        }
        if !graph_is_active(&scene.pass_nodes, &scene.draw_commands, *graph_index) {
            if let Some(timing) = gpu_timing {
                timing.record_graph_effect_target_finish(device, command_buffer, graph_position);
                timing.record_graph_scene_color_start(device, command_buffer, graph_position);
                timing.record_graph_scene_color_finish(device, command_buffer, graph_position);
                timing.record_graph_finish(device, command_buffer, graph_position);
            }
            continue;
        }
        let requires_effect_target_execution =
            effect_target::graph_requires_effect_target_execution(
                &scene.effect_target_commands,
                *graph_index,
            );
        if graph_requires_interleaved_target_execution(&scene.pass_nodes, *graph_index) {
            if scene_color_rendering_active {
                unsafe {
                    device.cmd_end_rendering(command_buffer);
                }
                scene_color_rendering_active = false;
            }
            record_interleaved_target_graph(
                device,
                command_buffer,
                swapchain_image,
                swapchain_view,
                extent,
                clear_color,
                scene,
                reference_slots,
                *graph_index,
                gpu_timing,
                &mut scene_color_initialized,
                &mut scene_color_rendering_active,
                &mut scene_color_resolve_dirty,
            )?;
            if let Some(timing) = gpu_timing {
                timing.record_graph_effect_target_finish(device, command_buffer, graph_position);
                timing.record_graph_scene_color_start(device, command_buffer, graph_position);
                timing.record_graph_scene_color_finish(device, command_buffer, graph_position);
                timing.record_graph_finish(device, command_buffer, graph_position);
            }
            continue;
        }
        if requires_effect_target_execution {
            if scene_color_rendering_active {
                unsafe {
                    device.cmd_end_rendering(command_buffer);
                }
                scene_color_rendering_active = false;
            }
            if !scene_color_initialized
                && effect_target::graph_copies_scene_color(
                    &scene.effect_target_commands,
                    *graph_index,
                )
            {
                begin_scene_color_rendering(
                    device,
                    command_buffer,
                    swapchain_view,
                    extent,
                    clear_color,
                    false,
                    scene,
                );
                unsafe {
                    device.cmd_end_rendering(command_buffer);
                }
                scene_color_initialized = true;
                scene_color_resolve_dirty = true;
            }
            let graph_copies_scene_color = effect_target::graph_copies_scene_color(
                &scene.effect_target_commands,
                *graph_index,
            );
            let direct_scene_color_snapshot = effect_target::graph_uses_direct_scene_color_snapshot(
                &scene.effect_target_commands,
                *graph_index,
            );
            if scene_color_resolve_dirty
                && (graph_copies_scene_color || direct_scene_color_snapshot)
            {
                resolve_explicit_scene_color_msaa(
                    device,
                    command_buffer,
                    swapchain_image,
                    extent,
                    scene,
                );
                scene_color_resolve_dirty = false;
            }
            if direct_scene_color_snapshot {
                transition_scene_color_to_sampled(device, command_buffer, swapchain_image);
            }
            let mut record_effect_draws = |draw_start, draw_count, target_extent| {
                record_scene_mesh_draw_ranges(
                    device,
                    command_buffer,
                    scene,
                    &[SceneGpuDrawRange {
                        start: draw_start,
                        count: draw_count,
                    }],
                    target_extent,
                )
            };
            let mut record_effect_command_timing = |source_position, starting| {
                if let Some(timing) = gpu_timing {
                    timing.record_effect_command(device, command_buffer, source_position, starting);
                }
            };
            effect_target::record_scene_effect_target_graph_passes(
                device,
                command_buffer,
                swapchain_image,
                extent,
                *graph_index,
                &scene.effect_target_commands,
                &scene.effect_target_allocations,
                reference_slots,
                &scene.effect_targets,
                &mut record_effect_draws,
                &mut record_effect_command_timing,
            )?;
            if direct_scene_color_snapshot {
                transition_scene_color_to_attachment(device, command_buffer, swapchain_image);
            }
        }

        if let Some(timing) = gpu_timing {
            timing.record_graph_effect_target_finish(device, command_buffer, graph_position);
            timing.record_graph_scene_color_start(device, command_buffer, graph_position);
        }

        let mut graph_ranges = scene
            .scene_color_draw_ranges
            .iter()
            .filter(|range| range.graph_index == *graph_index)
            .peekable();
        if graph_ranges.peek().is_some() {
            let attachment_clear = (!scene_color_initialized)
                .then_some(scene.scene_color_attachment_clear)
                .flatten()
                .filter(|clear| {
                    clear.graph_index == *graph_index
                        && scene
                            .scene_color_draw_ranges
                            .iter()
                            .copied()
                            .any(|range| clear.replaces(range))
                });
            if !scene_color_rendering_active {
                begin_scene_color_rendering(
                    device,
                    command_buffer,
                    swapchain_view,
                    extent,
                    attachment_clear.map_or(clear_color, |clear| clear.color),
                    scene_color_initialized,
                    scene,
                );
                record_scene_draw_extent(device, command_buffer, extent);
                scene_color_rendering_active = true;
            }
            for graph_range in graph_ranges {
                if attachment_clear.is_some_and(|clear| clear.replaces(*graph_range)) {
                    continue;
                }
                record_scene_mesh_draw_ranges(
                    device,
                    command_buffer,
                    scene,
                    &[graph_range.range],
                    extent,
                )?;
            }
            scene_color_initialized = true;
            scene_color_resolve_dirty = true;
        }
        if let Some(timing) = gpu_timing {
            timing.record_graph_scene_color_finish(device, command_buffer, graph_position);
            timing.record_graph_finish(device, command_buffer, graph_position);
        }
    }

    if scene_color_rendering_active {
        unsafe {
            device.cmd_end_rendering(command_buffer);
        }
    } else if !scene_color_initialized {
        begin_scene_color_rendering(
            device,
            command_buffer,
            swapchain_view,
            extent,
            clear_color,
            false,
            scene,
        );
        unsafe {
            device.cmd_end_rendering(command_buffer);
        }
        scene_color_resolve_dirty = true;
    }
    if scene_color_resolve_dirty {
        resolve_explicit_scene_color_msaa(device, command_buffer, swapchain_image, extent, scene);
    }
    Ok(())
}

fn graph_is_active(
    pass_nodes: &[SceneRenderingDevicePassNode],
    draw_commands: &[SceneGpuDrawCommand],
    graph_index: u32,
) -> bool {
    let activation_policy = pass_nodes
        .iter()
        .find(|pass| pass.graph_index == graph_index)
        .map(|pass| pass.graph_activation_policy)
        .unwrap_or(crate::engine::scene::SceneRenderGraphActivationPolicy::Always);
    if activation_policy == crate::engine::scene::SceneRenderGraphActivationPolicy::Always {
        return true;
    }
    pass_nodes
        .iter()
        .filter(|pass| pass.graph_index == graph_index)
        .flat_map(|pass| {
            let start = pass.mesh_draw_start as usize;
            let end = start.saturating_add(pass.mesh_draw_count as usize);
            draw_commands.get(start..end).into_iter().flatten()
        })
        .any(|draw| draw.enabled)
}

fn graph_requires_interleaved_target_execution(
    passes: &[SceneRenderingDevicePassNode],
    graph_index: u32,
) -> bool {
    let mut effect_target_seen = false;
    let mut scene_color_after_effect = false;
    for pass in passes.iter().filter(|pass| pass.graph_index == graph_index) {
        if pass_is_scene_color(pass) {
            scene_color_after_effect |= effect_target_seen;
        } else if pass_targets_effect_image(pass) {
            if scene_color_after_effect {
                return true;
            }
            effect_target_seen = true;
        }
    }
    false
}

fn record_interleaved_target_graph(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    swapchain_image: vk::Image,
    swapchain_view: vk::ImageView,
    extent: vk::Extent2D,
    clear_color: NativeVulkanClearColor,
    scene: &SceneGpuResources,
    reference_slots: &[u32],
    graph_index: u32,
    gpu_timing: Option<&SceneGpuTiming>,
    scene_color_initialized: &mut bool,
    scene_color_rendering_active: &mut bool,
    scene_color_resolve_dirty: &mut bool,
) -> Result<(), String> {
    for pass in scene
        .pass_nodes
        .iter()
        .filter(|pass| pass.graph_index == graph_index)
    {
        if pass_is_scene_color(pass) {
            if pass.mesh_draw_count == 0 {
                continue;
            }
            if !*scene_color_rendering_active {
                begin_scene_color_rendering(
                    device,
                    command_buffer,
                    swapchain_view,
                    extent,
                    clear_color,
                    *scene_color_initialized,
                    scene,
                );
                record_scene_draw_extent(device, command_buffer, extent);
                *scene_color_rendering_active = true;
            }
            record_scene_mesh_draw_ranges(
                device,
                command_buffer,
                scene,
                &[SceneGpuDrawRange {
                    start: pass.mesh_draw_start,
                    count: pass.mesh_draw_count,
                }],
                extent,
            )?;
            *scene_color_initialized = true;
            *scene_color_resolve_dirty = true;
            continue;
        }
        if !pass_targets_effect_image(pass) {
            continue;
        }
        if *scene_color_rendering_active {
            unsafe {
                device.cmd_end_rendering(command_buffer);
            }
            *scene_color_rendering_active = false;
        }
        if *scene_color_resolve_dirty {
            resolve_explicit_scene_color_msaa(
                device,
                command_buffer,
                swapchain_image,
                extent,
                scene,
            );
            *scene_color_resolve_dirty = false;
        }
        let mut record_draws = |draw_start, draw_count, target_extent| {
            record_scene_mesh_draw_ranges(
                device,
                command_buffer,
                scene,
                &[SceneGpuDrawRange {
                    start: draw_start,
                    count: draw_count,
                }],
                target_extent,
            )
        };
        let mut record_effect_command_timing = |source_position, starting| {
            if let Some(timing) = gpu_timing {
                timing.record_effect_command(device, command_buffer, source_position, starting);
            }
        };
        effect_target::record_scene_effect_target_pass(
            device,
            command_buffer,
            swapchain_image,
            extent,
            pass,
            &scene.effect_target_commands,
            &scene.effect_target_allocations,
            reference_slots,
            &scene.effect_targets,
            &mut record_draws,
            &mut record_effect_command_timing,
        )?;
    }
    Ok(())
}

fn pass_is_scene_color(pass: &SceneRenderingDevicePassNode) -> bool {
    matches!(
        pass.target,
        SceneRenderTargetKind::SceneColor | SceneRenderTargetKind::Swapchain
    )
}

fn pass_targets_effect_image(pass: &SceneRenderingDevicePassNode) -> bool {
    !pass_is_scene_color(pass)
}

fn begin_scene_color_rendering(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    swapchain_view: vk::ImageView,
    extent: vk::Extent2D,
    clear_color: NativeVulkanClearColor,
    initialized: bool,
    scene: &SceneGpuResources,
) {
    let clear_value = vk::ClearValue {
        color: vk::ClearColorValue {
            float32: [clear_color.r, clear_color.g, clear_color.b, clear_color.a],
        },
    };
    let explicit_msaa_target = scene.scene_color_msaa_targets.get(scene.active_frame_slot);
    let color_attachment = vk::RenderingAttachmentInfo::builder()
        .image_view(explicit_msaa_target.map_or(swapchain_view, |target| target.view))
        .image_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .load_op(if initialized {
            vk::AttachmentLoadOp::LOAD
        } else {
            vk::AttachmentLoadOp::CLEAR
        })
        .store_op(vk::AttachmentStoreOp::STORE)
        .clear_value(clear_value);
    let color_attachment = color_attachment.build();
    let color_attachments = [color_attachment];
    let mut multisampled_render_to_single_sampled =
        vk::MultisampledRenderToSingleSampledInfoEXT::builder()
            .multisampled_render_to_single_sampled_enable(true)
            .rasterization_samples(vk::SampleCountFlags::_4)
            .build();
    let mut rendering_info = vk::RenderingInfo::builder()
        .render_area(
            vk::Rect2D::builder()
                .offset(vk::Offset2D { x: 0, y: 0 })
                .extent(extent)
                .build(),
        )
        .layer_count(1)
        .color_attachments(&color_attachments);
    if scene.multisampled_render_to_single_sampled_enabled {
        rendering_info = rendering_info.push_next(&mut multisampled_render_to_single_sampled);
    }
    let rendering_info = rendering_info.build();
    unsafe {
        device.cmd_begin_rendering(command_buffer, &rendering_info);
    }
}

fn resolve_explicit_scene_color_msaa(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    swapchain_image: vk::Image,
    extent: vk::Extent2D,
    scene: &SceneGpuResources,
) {
    let Some(source) = scene.scene_color_msaa_targets.get(scene.active_frame_slot) else {
        return;
    };
    let to_transfer = [
        scene_color_resolve_barrier(
            source.image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags2::ALL_TRANSFER,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            vk::AccessFlags2::TRANSFER_READ,
        ),
        scene_color_resolve_barrier(
            swapchain_image,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::PipelineStageFlags2::ALL_TRANSFER,
            vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
            vk::AccessFlags2::TRANSFER_WRITE,
        ),
    ];
    let transfer_dependency = vk::DependencyInfo::builder()
        .image_memory_barriers(&to_transfer)
        .build();
    let resolve = vk::ImageResolve::builder()
        .src_subresource(
            vk::ImageSubresourceLayers::builder()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .layer_count(1)
                .build(),
        )
        .dst_subresource(
            vk::ImageSubresourceLayers::builder()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .layer_count(1)
                .build(),
        )
        .extent(vk::Extent3D {
            width: extent.width,
            height: extent.height,
            depth: 1,
        })
        .build();
    let to_attachment = [
        scene_color_resolve_barrier(
            source.image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::PipelineStageFlags2::ALL_TRANSFER,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::TRANSFER_READ,
            vk::AccessFlags2::COLOR_ATTACHMENT_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        ),
        scene_color_resolve_barrier(
            swapchain_image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            vk::PipelineStageFlags2::ALL_TRANSFER,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            vk::AccessFlags2::TRANSFER_WRITE,
            vk::AccessFlags2::COLOR_ATTACHMENT_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        ),
    ];
    let attachment_dependency = vk::DependencyInfo::builder()
        .image_memory_barriers(&to_attachment)
        .build();
    unsafe {
        device.cmd_pipeline_barrier2(command_buffer, &transfer_dependency);
        device.cmd_resolve_image(
            command_buffer,
            source.image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            swapchain_image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[resolve],
        );
        device.cmd_pipeline_barrier2(command_buffer, &attachment_dependency);
    }
}

#[allow(clippy::too_many_arguments)]
fn scene_color_resolve_barrier(
    image: vk::Image,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
    src_stage: vk::PipelineStageFlags2,
    dst_stage: vk::PipelineStageFlags2,
    src_access: vk::AccessFlags2,
    dst_access: vk::AccessFlags2,
) -> vk::ImageMemoryBarrier2 {
    vk::ImageMemoryBarrier2::builder()
        .src_stage_mask(src_stage)
        .src_access_mask(src_access)
        .dst_stage_mask(dst_stage)
        .dst_access_mask(dst_access)
        .old_layout(old_layout)
        .new_layout(new_layout)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(color_subresource_range())
        .build()
}

fn transition_scene_color_msaa_to_attachment(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    scene: &SceneGpuResources,
) {
    let Some(target) = scene.scene_color_msaa_targets.get(scene.active_frame_slot) else {
        return;
    };
    let barrier = vk::ImageMemoryBarrier2::builder()
        .src_stage_mask(vk::PipelineStageFlags2::TOP_OF_PIPE)
        .src_access_mask(vk::AccessFlags2::empty())
        .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        .dst_access_mask(
            vk::AccessFlags2::COLOR_ATTACHMENT_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        )
        .old_layout(vk::ImageLayout::UNDEFINED)
        .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(target.image)
        .subresource_range(color_subresource_range())
        .build();
    let barriers = [barrier];
    let dependency = vk::DependencyInfo::builder()
        .image_memory_barriers(&barriers)
        .build();
    unsafe {
        device.cmd_pipeline_barrier2(command_buffer, &dependency);
    }
}

fn transition_swapchain_to_attachment(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
    old_layout: vk::ImageLayout,
) {
    let barrier = vk::ImageMemoryBarrier2::builder()
        .src_stage_mask(match old_layout {
            vk::ImageLayout::UNDEFINED => vk::PipelineStageFlags2::TOP_OF_PIPE,
            _ => vk::PipelineStageFlags2::ALL_COMMANDS,
        })
        .src_access_mask(vk::AccessFlags2::empty())
        .dst_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        .dst_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
        .old_layout(old_layout)
        .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(color_subresource_range())
        .build();
    unsafe {
        device.cmd_pipeline_barrier2(
            command_buffer,
            &vk::DependencyInfo::builder()
                .image_memory_barriers(&[barrier])
                .build(),
        );
    }
}

fn transition_scene_color_to_sampled(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
) {
    transition_scene_color_layout(
        device,
        command_buffer,
        image,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        vk::PipelineStageFlags2::FRAGMENT_SHADER,
        vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        vk::AccessFlags2::SHADER_SAMPLED_READ,
    );
}

fn transition_scene_color_to_attachment(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
) {
    transition_scene_color_layout(
        device,
        command_buffer,
        image,
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
        vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        vk::PipelineStageFlags2::FRAGMENT_SHADER,
        vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
        vk::AccessFlags2::SHADER_SAMPLED_READ,
        vk::AccessFlags2::COLOR_ATTACHMENT_READ | vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
    );
}

#[allow(clippy::too_many_arguments)]
fn transition_scene_color_layout(
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
        .subresource_range(color_subresource_range())
        .build();
    unsafe {
        device.cmd_pipeline_barrier2(
            command_buffer,
            &vk::DependencyInfo::builder()
                .image_memory_barriers(&[barrier])
                .build(),
        );
    }
}

pub(super) fn transition_swapchain_to_present(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    image: vk::Image,
) {
    let barrier = vk::ImageMemoryBarrier2::builder()
        .src_stage_mask(vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT)
        .src_access_mask(vk::AccessFlags2::COLOR_ATTACHMENT_WRITE)
        .dst_stage_mask(vk::PipelineStageFlags2::BOTTOM_OF_PIPE)
        .dst_access_mask(vk::AccessFlags2::empty())
        .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .new_layout(vk::ImageLayout::PRESENT_SRC_KHR)
        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
        .image(image)
        .subresource_range(color_subresource_range())
        .build();
    unsafe {
        device.cmd_pipeline_barrier2(
            command_buffer,
            &vk::DependencyInfo::builder()
                .image_memory_barriers(&[barrier])
                .build(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{SceneRenderingDeviceGraphPlan, SceneRenderingDevicePassNode};

    #[test]
    fn execution_order_keeps_graphs_contiguous() {
        let graph = SceneRenderingDeviceGraphPlan {
            pass_nodes: vec![pass(0), pass(0), pass(2)],
            ..empty_graph()
        };
        assert_eq!(scene_graph_execution_order(&graph), vec![0, 2]);
    }

    #[test]
    fn execution_order_drops_invisible_zero_draw_graphs() {
        let mut invisible = pass(1);
        invisible.mesh_draw_count = 0;
        let graph = SceneRenderingDeviceGraphPlan {
            pass_nodes: vec![pass(0), invisible, pass(2)],
            ..empty_graph()
        };

        assert_eq!(scene_graph_execution_order(&graph), vec![0, 2]);
    }

    #[test]
    fn repeated_effect_target_and_scene_color_runs_require_pass_order_execution() {
        let scene = pass(7);
        let mut effect = pass(7);
        effect.target = SceneRenderTargetKind::FirstClassEffectTarget;
        effect.target_name = crate::engine::scene::SceneStringId(4);
        let ordered = vec![scene, effect, scene, effect];
        assert!(graph_requires_interleaved_target_execution(&ordered, 7));

        let grouped = vec![effect, effect, scene];
        assert!(!graph_requires_interleaved_target_execution(&grouped, 7));
    }

    #[test]
    fn only_effect_gated_graphs_are_skipped_without_an_enabled_draw() {
        let mut always = pass(4);
        always.mesh_draw_count = 0;
        assert!(graph_is_active(&[always], &[], 4));

        let mut effect_gated = pass(5);
        effect_gated.graph_activation_policy =
            crate::engine::scene::SceneRenderGraphActivationPolicy::AnyEffectVisible;
        effect_gated.mesh_draw_count = 0;
        assert!(!graph_is_active(&[effect_gated], &[], 5));
    }

    fn pass(graph_index: u32) -> SceneRenderingDevicePassNode {
        SceneRenderingDevicePassNode {
            graph_index,
            graph_activation_policy:
                crate::engine::scene::SceneRenderGraphActivationPolicy::Always,
            pass_record_index: 0,
            pass_id: 0,
            role: crate::engine::scene::SceneRenderPassKind::BaseMaterial,
            target: crate::engine::scene::SceneRenderTargetKind::SceneColor,
            target_name: crate::engine::scene::SceneStringId::NONE,
            binding_start: 0,
            binding_count: 0,
            effect_binding_start: u32::MAX,
            effect_binding_count: 0,
            effect_visibility_policy:
                crate::engine::scene::SceneRenderEffectVisibilityPolicy::None,
            mesh_draw_start: 0,
            mesh_draw_count: 1,
        }
    }

    fn empty_graph() -> SceneRenderingDeviceGraphPlan {
        SceneRenderingDeviceGraphPlan {
            pass_nodes: Vec::new(),
            target_allocations: Vec::new(),
            effect_batches: Vec::new(),
            effect_batch_instances: Vec::new(),
            sampled_bindings: Vec::new(),
            material_sampled_bindings: Vec::new(),
            mesh_draws: Vec::new(),
            puppet_bone_palettes: Vec::new(),
            puppet_bone_matrices: Vec::new(),
            particle_gpu_emitters: Vec::new(),
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
