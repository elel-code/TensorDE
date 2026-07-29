//! Scene mesh draw recording shared by swapchain and effect-target passes.
//!
//! References:
//! - `docs/gilder/gilder-scene-engine-architecture.md`
//! - `references/gilder/godot/servers/rendering/rendering_device_graph.*`

use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk::{self, ExtDescriptorHeapExtensionDeviceCommands};

use crate::engine::scene::{
    SceneRenderTargetKind, SceneRenderingDeviceDrawPrimitive, SceneRenderingDeviceGraphPlan,
};
use crate::renderer::native_vulkan::{
    native_vulkan_vulkanalia_descriptor_heap_mixed_resource_bind_info_for_descriptor,
    native_vulkan_vulkanalia_descriptor_heap_mixed_sampler_bind_info_for_descriptor,
};

use super::{SceneGpuResources, native_descriptor_push::SceneNativeFragmentPush};

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
    pub enabled: bool,
    pub primitive: SceneRenderingDeviceDrawPrimitive,
    pub pipeline_index: u32,
    pub authored_pipeline_index: u32,
    pub disabled_pipeline_index: Option<u32>,
    pub first_index: u32,
    pub index_count: u32,
    pub vertex_offset: i32,
    pub vertex_count: u32,
    pub instance_count: u32,
    pub instance_capacity: u32,
    pub first_instance: u32,
    pub dynamic_text: bool,
    pub particle_indirect_index: Option<u32>,
    pub resource_descriptor_base: usize,
    pub material_resource_descriptor: Option<usize>,
    pub skinning_resource_descriptor: Option<usize>,
    pub sampled_resource_descriptor_base: usize,
    pub input_attachment_resource_descriptor_base: usize,
    pub sampler_descriptor_base: usize,
    pub native_fragment_push: Option<SceneNativeFragmentPush>,
    pub skinning_byte_offset: u64,
    pub skinning_byte_count: u64,
    pub scissor: Option<SceneGpuScissor>,
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
        let vertex_buffers = [scene.mesh_uploads.vertex.target.buffer];
        let vertex_offsets = [0u64];
        device.cmd_bind_vertex_buffers(command_buffer, 0, &vertex_buffers, &vertex_offsets);
        device.cmd_bind_index_buffer(
            command_buffer,
            scene.mesh_uploads.index.target.buffer,
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
        if !draw.enabled {
            continue;
        }
        let frame = scene.active_frame();
        let resource_bind =
            native_vulkan_vulkanalia_descriptor_heap_mixed_resource_bind_info_for_descriptor(
                &frame.descriptor_heap,
                draw.resource_descriptor_base,
            )?;
        unsafe {
            device.cmd_bind_resource_heap_ext(command_buffer, &resource_bind);
        }
        if !scene.descriptor_layout.sampled_slots.is_empty() {
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
            if let Some(push) = draw.native_fragment_push {
                let bytes = push.bytes();
                let range = vk::HostAddressRangeConstEXT::builder().address(&bytes);
                let info = vk::PushDataInfoEXT::builder().offset(0).data(range);
                device.cmd_push_data_ext(command_buffer, &info);
            }
            if draw.dynamic_text {
                let instance_buffer = [frame.transform_buffer.buffer];
                let instance_offset =
                    [scene.draw_commands.len() as u64 * super::SCENE_DRAW_UNIFORM_BYTES];
                device.cmd_bind_vertex_buffers(
                    command_buffer,
                    1,
                    &instance_buffer,
                    &instance_offset,
                );
            }
        }
        let scissor = scene_vk_scissor(draw.scissor, extent);
        unsafe {
            device.cmd_set_scissor(command_buffer, 0, &[scissor]);
            record_bound_scene_draw(device, command_buffer, scene, draw);
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
    if draw.dynamic_text {
        unsafe {
            device.cmd_draw(
                command_buffer,
                6,
                draw.instance_count,
                0,
                draw.first_instance,
            );
        }
        return;
    }
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
