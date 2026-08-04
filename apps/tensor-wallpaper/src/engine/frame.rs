use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameClock {
    pub scene_time_ms: u64,
    pub media_time_ms: Option<u64>,
    pub audio_master_time_ms: Option<u64>,
    pub delta_time_ms: u32,
    pub paused: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameDirtySet {
    pub object_transform_dirty: u32,
    pub material_constants_dirty: u32,
    pub resource_binding_dirty: u32,
    pub graph_topology_dirty: bool,
    pub geometry_topology_dirty: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameGpuUpdates {
    pub vertex_upload_bytes: u64,
    pub index_upload_bytes: u64,
    pub constant_upload_bytes: u64,
    pub pose_upload_bytes: u64,
    pub particle_upload_bytes: u64,
    pub descriptor_heap_rewrites: u32,
}
