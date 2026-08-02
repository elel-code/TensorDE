//! Cold authored graph execution order shared by retained recorders.

use crate::engine::scene::{SceneRenderingDeviceGraphPlan, SceneRenderingDevicePassNode};

pub(super) fn scene_graph_execution_order(graph: &SceneRenderingDeviceGraphPlan) -> Vec<u32> {
    graph_order(graph.pass_nodes.iter())
}

pub(super) fn scene_graph_draw_execution_order(graph: &SceneRenderingDeviceGraphPlan) -> Vec<u32> {
    graph_order(
        graph
            .pass_nodes
            .iter()
            .filter(|pass| pass.mesh_draw_count != 0),
    )
}

fn graph_order<'a>(passes: impl Iterator<Item = &'a SceneRenderingDevicePassNode>) -> Vec<u32> {
    let mut order = Vec::new();
    for pass in passes {
        if order.last().copied() != Some(pass.graph_index) {
            order.push(pass.graph_index);
        }
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{
        SceneRenderEffectVisibilityPolicy, SceneRenderGraphActivationPolicy, SceneRenderPassKind,
        SceneRenderTargetKind, SceneRenderingDevicePassNode, SceneStringId,
    };

    #[test]
    fn retains_zero_draw_copy_graphs_in_authored_order() {
        let graph = SceneRenderingDeviceGraphPlan {
            pass_nodes: vec![
                pass(0, SceneRenderPassKind::CopyTarget, 0),
                pass(1, SceneRenderPassKind::BaseMaterial, 1),
            ],
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
        };

        assert_eq!(scene_graph_execution_order(&graph), vec![0, 1]);
        assert_eq!(scene_graph_draw_execution_order(&graph), vec![1]);
    }

    fn pass(
        graph_index: u32,
        role: SceneRenderPassKind,
        mesh_draw_count: u32,
    ) -> SceneRenderingDevicePassNode {
        SceneRenderingDevicePassNode {
            graph_index,
            graph_activation_policy: SceneRenderGraphActivationPolicy::Always,
            pass_record_index: graph_index,
            pass_id: 0,
            role,
            target: if role == SceneRenderPassKind::CopyTarget {
                SceneRenderTargetKind::FirstClassEffectTarget
            } else {
                SceneRenderTargetKind::SceneColor
            },
            target_name: SceneStringId::NONE,
            binding_start: 0,
            binding_count: 0,
            effect_binding_start: u32::MAX,
            effect_binding_count: 0,
            effect_visibility_policy: SceneRenderEffectVisibilityPolicy::None,
            mesh_draw_start: 0,
            mesh_draw_count,
        }
    }
}
