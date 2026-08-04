//! Scene mesh draw recording shared by swapchain and effect-target passes.
//!
//! References:
//! - `docs/tensor-wallpaper/tensor-wallpaper-scene-engine-architecture.md`
//! - `references/tensor-wallpaper/godot/servers/rendering/rendering_device_graph.*`

use super::descriptor_push::SceneDescriptorPush;
use crate::engine::scene::{
    SceneRenderTargetKind, SceneRenderingDeviceDrawPrimitive, SceneRenderingDeviceGraphPlan,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::rendering_device) struct SceneGpuDrawRange {
    pub start: u32,
    pub count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::rendering_device) struct SceneGpuGraphDrawRange {
    pub graph_index: u32,
    pub range: SceneGpuDrawRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::renderer::rendering_device) struct SceneGpuScissor {
    pub offset: [i32; 2],
    pub extent: [u32; 2],
}

#[derive(Debug, Clone)]
pub(in crate::renderer::rendering_device) struct SceneGpuDrawCommand {
    pub enabled: bool,
    pub primitive: SceneRenderingDeviceDrawPrimitive,
    pub pipeline_index: u32,
    pub authored_pipeline_index: u32,
    pub disabled_pipeline_index: Option<u32>,
    pub first_index: u32,
    pub index_count: u32,
    pub vertex_offset: i32,
    pub vertex_buffer_byte_offset: Option<u64>,
    pub vertex_count: u32,
    pub instance_count: u32,
    pub instance_capacity: u32,
    pub first_instance: u32,
    pub dynamic_text: bool,
    /// Typed graph-selected decoder identity for an exact scene-video draw.
    pub video_media_instance: Option<u32>,
    pub video_vertex_byte_offset: Option<u64>,
    pub particle_indirect_index: Option<u32>,
    pub resource_descriptor_base: usize,
    pub material_resource_descriptor: Option<usize>,
    pub skinning_resource_descriptor: Option<usize>,
    pub particle_resource_descriptor: Option<usize>,
    pub scene_owned_uniform_descriptor_base: usize,
    pub sampled_resource_descriptor_base: usize,
    pub input_attachment_resource_descriptor_base: usize,
    pub sampler_descriptor_base: usize,
    pub descriptor_push: Option<SceneDescriptorPush>,
    pub disabled_descriptor_push: Option<SceneDescriptorPush>,
    pub skinning_byte_offset: u64,
    pub skinning_byte_count: u64,
    pub scissor: Option<SceneGpuScissor>,
}

impl SceneGpuDrawCommand {
    pub(super) fn active_descriptor_push(&self) -> Option<&SceneDescriptorPush> {
        if self.disabled_pipeline_index == Some(self.pipeline_index) {
            self.disabled_descriptor_push.as_ref()
        } else {
            self.descriptor_push.as_ref()
        }
    }
}

pub(in crate::renderer::rendering_device) fn scene_color_draw_ranges(
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

pub(in crate::renderer::rendering_device) fn draw_range_count(
    ranges: &[SceneGpuGraphDrawRange],
) -> usize {
    ranges.iter().map(|range| range.range.count as usize).sum()
}
