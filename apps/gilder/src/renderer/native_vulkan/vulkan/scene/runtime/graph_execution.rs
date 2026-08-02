//! Cold authored graph execution order shared by retained recorders.

use crate::engine::scene::SceneRenderingDeviceGraphPlan;

pub(super) fn scene_graph_execution_order(graph: &SceneRenderingDeviceGraphPlan) -> Vec<u32> {
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
