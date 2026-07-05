use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SceneEngineTelemetry {
    pub render_graph_passes: u32,
    pub unsupported_graph_boundaries: u32,
    pub render_graph_resource_uses: u32,
    pub render_graph_derived_barriers: u32,
    pub render_graph_execution_dependencies: u32,
    pub render_graph_execution_levels: u32,
    pub render_graph_logical_targets: u32,
    pub render_graph_physical_target_slots: u32,
    pub render_graph_aliased_targets: u32,
    pub retained_layer_pose_timeline_bytes: u64,
    pub retained_layer_pose_timeline_layers: u32,
    pub retained_layer_pose_timeline_frames: u32,
    pub retained_layer_pose_timeline_model: &'static str,
}
