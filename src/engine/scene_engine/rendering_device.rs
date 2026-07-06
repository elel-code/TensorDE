//! Godot-aligned RenderingDevice boundary.
//!
//! References:
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/rendering_device.h`

use super::{
    SceneFramePlan, SceneGeometryId, SceneGraph, SceneObjectId, ScenePuppetId, SceneResourceId,
    SceneResourceResidencyPlan,
};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum RenderingDeviceCommand {
    BeginPass {
        name: String,
    },
    BindPipeline {
        name: String,
    },
    BindTexture {
        slot: u32,
        resource: SceneResourceId,
    },
    EnsureTextureResident {
        resource: SceneResourceId,
        width: Option<u32>,
        height: Option<u32>,
    },
    EnsureBufferResident {
        resource: SceneResourceId,
        bytes: u64,
    },
    EnsureMeshGeometryResident {
        geometry: SceneGeometryId,
        source_record: u32,
        vertex_count: u32,
        index_count: u32,
        vertex_bytes: u64,
        index_bytes: u64,
    },
    EnsurePuppetRigResident {
        puppet: ScenePuppetId,
        source_record: u32,
        bone_count: u32,
        skin_vertex_count: u32,
        attachment_count: u32,
        clip_count: u32,
        clip_bone_count: u32,
        clip_frame_count: u32,
        clip_frame_bytes: u64,
        layer_count: u32,
        clipping_record_count: u32,
        clipping_bone_count: u32,
        clipping_frame_key_count: u32,
    },
    DrawIndexed {
        object: SceneObjectId,
        geometry: Option<SceneGeometryId>,
        puppet: Option<ScenePuppetId>,
        index_count: u32,
    },
    EndPass,
}

pub trait RenderingDevice {
    fn record_scene_frame(&mut self, frame: &SceneFramePlan);
    fn record_resource_residency(&mut self, residency: &SceneResourceResidencyPlan);
    fn record_scene_graph(&mut self, graph: &SceneGraph);
    fn commands(&self) -> &[RenderingDeviceCommand];
}
