//! Pipeline-key projection for retained local-read scope roles.

use crate::engine::scene::SceneRenderingDevicePassNode;

use super::super::local_read::SceneLocalReadScopePlan;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ScenePipelineLocalReadRole {
    Producer(usize),
    Consumer(usize),
}

pub(super) fn local_read_pipeline_role(
    local_read_scopes: &[SceneLocalReadScopePlan],
    pass: &SceneRenderingDevicePassNode,
    _passthrough: bool,
) -> Result<Option<ScenePipelineLocalReadRole>, String> {
    for (scope_index, scope) in local_read_scopes.iter().enumerate() {
        let producer_matches = scope.producer_pass_record_index() == pass.pass_record_index
            && scope.producer_draw_range() == (pass.mesh_draw_start, pass.mesh_draw_count)
            && scope.graph_index() == pass.graph_index;
        let consumer_matches = scope.consumer_pass_record_index() == pass.pass_record_index
            && scope.consumer_draw_range() == (pass.mesh_draw_start, pass.mesh_draw_count)
            && scope.graph_index() == pass.graph_index;
        if producer_matches {
            return Ok(Some(ScenePipelineLocalReadRole::Producer(scope_index)));
        }
        if consumer_matches {
            return Ok(Some(ScenePipelineLocalReadRole::Consumer(scope_index)));
        }
    }
    Ok(None)
}
