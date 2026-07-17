//! Scene mesh draw recording shared by swapchain and effect-target passes.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `references/godot/servers/rendering/rendering_device_graph.*`

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, ExtDescriptorHeapExtensionDeviceCommands};

use crate::engine::scene::{
    SceneRenderTargetKind, SceneRenderingDeviceDrawPrimitive, SceneRenderingDeviceGraphPlan,
};
use crate::renderer::native_vulkan::{
    native_vulkan_vulkanalia_descriptor_heap_mixed_resource_bind_info_for_descriptor,
    native_vulkan_vulkanalia_descriptor_heap_mixed_sampler_bind_info_for_descriptor,
};

use super::SceneGpuResources;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct SceneGpuDrawRange {
    pub start: u32,
    pub count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct SceneGpuGraphDrawRange {
    pub graph_index: u32,
    pub range: SceneGpuDrawRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct SceneGpuScissor {
    pub offset: [i32; 2],
    pub extent: [u32; 2],
}

#[derive(Debug, Clone)]
pub(in crate::renderer::native_vulkan) struct SceneGpuDrawCommand {
    pub primitive: SceneRenderingDeviceDrawPrimitive,
    pub pipeline_index: u32,
    pub first_index: u32,
    pub index_count: u32,
    pub vertex_offset: i32,
    pub vertex_count: u32,
    pub instance_count: u32,
    pub instance_capacity: u32,
    pub particle_indirect_index: Option<u32>,
    pub resource_descriptor_base: usize,
    pub sampler_descriptor_base: usize,
    pub skinning_byte_offset: u64,
    pub skinning_byte_count: u64,
    pub scissor: Option<SceneGpuScissor>,
    pub alpha_coverage_scissors: Vec<SceneGpuScissor>,
}

pub(in crate::renderer::native_vulkan) fn scene_color_draw_ranges(
    graph: &SceneRenderingDeviceGraphPlan,
) -> Vec<SceneGpuGraphDrawRange> {
    graph
        .pass_nodes
        .iter()
        .filter(|pass| {
            pass.mesh_draw_count != 0
                && matches!(
                    pass.target,
                    SceneRenderTargetKind::SceneColor | SceneRenderTargetKind::Swapchain
                )
        })
        .map(|pass| SceneGpuGraphDrawRange {
            graph_index: pass.graph_index,
            range: SceneGpuDrawRange {
                start: pass.mesh_draw_start,
                count: pass.mesh_draw_count,
            },
        })
        .collect()
}

pub(in crate::renderer::native_vulkan) fn draw_range_count(
    ranges: &[SceneGpuGraphDrawRange],
) -> usize {
    ranges.iter().map(|range| range.range.count as usize).sum()
}

pub(in crate::renderer::native_vulkan) fn record_scene_draw_extent(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    extent: vk::Extent2D,
) {
    let viewport = vk::Viewport::builder()
        .x(0.0)
        .y(0.0)
        .width(extent.width as f32)
        .height(extent.height as f32)
        .min_depth(0.0)
        .max_depth(1.0)
        .build();
    let scissor = vk::Rect2D::builder()
        .offset(vk::Offset2D { x: 0, y: 0 })
        .extent(extent)
        .build();
    unsafe {
        device.cmd_set_viewport(command_buffer, 0, &[viewport]);
        device.cmd_set_scissor(command_buffer, 0, &[scissor]);
    }
}

pub(in crate::renderer::native_vulkan) fn record_scene_mesh_draw_ranges(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    scene: &SceneGpuResources,
    ranges: &[SceneGpuDrawRange],
    extent: vk::Extent2D,
) -> Result<(), String> {
    if ranges.is_empty() {
        return Ok(());
    }
    unsafe {
        let vertex_buffers = [scene.vertex_buffer.buffer];
        let vertex_offsets = [0u64];
        device.cmd_bind_vertex_buffers(command_buffer, 0, &vertex_buffers, &vertex_offsets);
        device.cmd_bind_index_buffer(
            command_buffer,
            scene.index_buffer.buffer,
            0,
            vk::IndexType::UINT32,
        );
        for range in ranges {
            record_scene_mesh_draws(device, command_buffer, scene, *range, extent)?;
        }
    }
    Ok(())
}

pub(in crate::renderer::native_vulkan) fn record_scene_mesh_draws(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    scene: &SceneGpuResources,
    range: SceneGpuDrawRange,
    extent: vk::Extent2D,
) -> Result<(), String> {
    let start = range.start as usize;
    let count = range.count as usize;
    let end = start.saturating_add(count);
    for draw in scene.draw_commands.get(start..end).unwrap_or(&[]) {
        let frame = scene.active_frame();
        let resource_bind =
            native_vulkan_vulkanalia_descriptor_heap_mixed_resource_bind_info_for_descriptor(
                &frame.descriptor_heap,
                draw.resource_descriptor_base,
            )?;
        unsafe {
            device.cmd_bind_resource_heap_ext(command_buffer, &resource_bind);
        }
        if !scene.sampled_slots.is_empty() {
            let sampler_bind =
                native_vulkan_vulkanalia_descriptor_heap_mixed_sampler_bind_info_for_descriptor(
                    &frame.descriptor_heap,
                    draw.sampler_descriptor_base,
                )?;
            unsafe {
                device.cmd_bind_sampler_heap_ext(command_buffer, &sampler_bind);
            }
        }
        unsafe {
            let pipeline = scene
                .pipelines
                .entries
                .get(draw.pipeline_index as usize)
                .ok_or_else(|| {
                    format!(
                        "scene draw references missing pipeline {}",
                        draw.pipeline_index
                    )
                })?
                .pipeline;
            device.cmd_bind_pipeline(command_buffer, vk::PipelineBindPoint::GRAPHICS, pipeline);
        }
        if draw.alpha_coverage_scissors.is_empty() {
            let scissor = scene_vk_scissor(draw.scissor, extent);
            unsafe {
                device.cmd_set_scissor(command_buffer, 0, &[scissor]);
                record_bound_scene_draw(device, command_buffer, scene, draw);
            }
            continue;
        }
        for coverage in &draw.alpha_coverage_scissors {
            let Some(scissor) = intersect_scissors(draw.scissor, *coverage, extent) else {
                continue;
            };
            unsafe {
                device.cmd_set_scissor(command_buffer, 0, &[scissor]);
                record_bound_scene_draw(device, command_buffer, scene, draw);
            }
        }
    }
    Ok(())
}

unsafe fn record_bound_scene_draw(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    scene: &SceneGpuResources,
    draw: &SceneGpuDrawCommand,
) {
    match draw.primitive {
        SceneRenderingDeviceDrawPrimitive::ObjectMesh => unsafe {
            device.cmd_draw_indexed(
                command_buffer,
                draw.index_count,
                1,
                draw.first_index,
                draw.vertex_offset,
                0,
            );
        },
        SceneRenderingDeviceDrawPrimitive::FullscreenTriangle
        | SceneRenderingDeviceDrawPrimitive::ObjectUvSupportQuad
        | SceneRenderingDeviceDrawPrimitive::ParticleBillboard => unsafe {
            if let Some(index) = draw.particle_indirect_index {
                let resources = scene
                    .particle_resources
                    .as_ref()
                    .expect("particle indirect draw requires GPU resources");
                device.cmd_draw_indirect(
                    command_buffer,
                    resources.indirect_upload.target.buffer,
                    u64::from(index) * 16,
                    1,
                    16,
                );
            } else {
                device.cmd_draw(command_buffer, draw.vertex_count, draw.instance_count, 0, 0);
            }
        },
    }
}

fn intersect_scissors(
    base: Option<SceneGpuScissor>,
    coverage: SceneGpuScissor,
    extent: vk::Extent2D,
) -> Option<vk::Rect2D> {
    let base = base.unwrap_or(SceneGpuScissor {
        offset: [0, 0],
        extent: [extent.width, extent.height],
    });
    let min_x = base.offset[0].max(coverage.offset[0]);
    let min_y = base.offset[1].max(coverage.offset[1]);
    let max_x = (base.offset[0] + base.extent[0] as i32)
        .min(coverage.offset[0] + coverage.extent[0] as i32)
        .min(extent.width as i32);
    let max_y = (base.offset[1] + base.extent[1] as i32)
        .min(coverage.offset[1] + coverage.extent[1] as i32)
        .min(extent.height as i32);
    (max_x > min_x && max_y > min_y).then(|| {
        vk::Rect2D::builder()
            .offset(vk::Offset2D { x: min_x, y: min_y })
            .extent(vk::Extent2D {
                width: (max_x - min_x) as u32,
                height: (max_y - min_y) as u32,
            })
            .build()
    })
}

fn scene_vk_scissor(scissor: Option<SceneGpuScissor>, extent: vk::Extent2D) -> vk::Rect2D {
    scissor.map_or_else(
        || {
            vk::Rect2D::builder()
                .offset(vk::Offset2D { x: 0, y: 0 })
                .extent(extent)
                .build()
        },
        |scissor| {
            vk::Rect2D::builder()
                .offset(vk::Offset2D {
                    x: scissor.offset[0],
                    y: scissor.offset[1],
                })
                .extent(vk::Extent2D {
                    width: scissor.extent[0],
                    height: scissor.extent[1],
                })
                .build()
        },
    )
}
