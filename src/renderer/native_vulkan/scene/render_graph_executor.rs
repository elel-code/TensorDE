//! Native Vulkan scene render graph executor planning.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device_graph.*`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.*`

use serde::Serialize;

use crate::engine::scene::{
    SceneRenderPassKind, SceneRenderTargetKind, SceneRenderingDeviceGraphPlan,
};

use super::pipeline_cache::NativeVulkanScenePipelineCachePlan;
use super::resource_storage::NativeVulkanSceneResourceStoragePlan;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneRenderGraphExecutorPlan {
    pub command_count: usize,
    pub draw_count: usize,
    pub commands: Vec<NativeVulkanSceneRenderGraphCommand>,
    pub heap_binding_model: &'static str,
    pub present_mode: &'static str,
    pub executor_status: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct NativeVulkanSceneRenderGraphCommand {
    pub pass_node_index: u32,
    pub kind: NativeVulkanSceneRenderGraphCommandKind,
    pub pass_role: SceneRenderPassKind,
    pub target: SceneRenderTargetKind,
    pub mesh_draw_start: u32,
    pub mesh_draw_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeVulkanSceneRenderGraphCommandKind {
    BeginTarget,
    BindDescriptorHeap,
    BindPipeline,
    DrawMesh,
    EndTarget,
}

pub fn native_vulkan_scene_render_graph_executor_plan(
    graph: &SceneRenderingDeviceGraphPlan,
    resource_storage: &NativeVulkanSceneResourceStoragePlan,
    pipeline_cache: &NativeVulkanScenePipelineCachePlan,
) -> NativeVulkanSceneRenderGraphExecutorPlan {
    let mut commands = Vec::new();
    for (index, pass) in graph.pass_nodes.iter().enumerate() {
        commands.push(command(
            index,
            NativeVulkanSceneRenderGraphCommandKind::BeginTarget,
            pass.role,
            pass.target,
            pass.mesh_draw_start,
            pass.mesh_draw_count,
        ));
        commands.push(command(
            index,
            NativeVulkanSceneRenderGraphCommandKind::BindDescriptorHeap,
            pass.role,
            pass.target,
            pass.mesh_draw_start,
            pass.mesh_draw_count,
        ));
        commands.push(command(
            index,
            NativeVulkanSceneRenderGraphCommandKind::BindPipeline,
            pass.role,
            pass.target,
            pass.mesh_draw_start,
            pass.mesh_draw_count,
        ));
        if pass.mesh_draw_count > 0 {
            commands.push(command(
                index,
                NativeVulkanSceneRenderGraphCommandKind::DrawMesh,
                pass.role,
                pass.target,
                pass.mesh_draw_start,
                pass.mesh_draw_count,
            ));
        }
        commands.push(command(
            index,
            NativeVulkanSceneRenderGraphCommandKind::EndTarget,
            pass.role,
            pass.target,
            pass.mesh_draw_start,
            pass.mesh_draw_count,
        ));
    }

    let draw_count = graph.mesh_draws.len();
    NativeVulkanSceneRenderGraphExecutorPlan {
        command_count: commands.len(),
        draw_count,
        commands,
        heap_binding_model: resource_storage.descriptor_heap.descriptor_model,
        present_mode: "fifo-latest-ready",
        executor_status: executor_status(resource_storage, pipeline_cache, draw_count),
    }
}

fn command(
    pass_node_index: usize,
    kind: NativeVulkanSceneRenderGraphCommandKind,
    pass_role: SceneRenderPassKind,
    target: SceneRenderTargetKind,
    mesh_draw_start: u32,
    mesh_draw_count: u32,
) -> NativeVulkanSceneRenderGraphCommand {
    NativeVulkanSceneRenderGraphCommand {
        pass_node_index: pass_node_index as u32,
        kind,
        pass_role,
        target,
        mesh_draw_start,
        mesh_draw_count,
    }
}

fn executor_status(
    resource_storage: &NativeVulkanSceneResourceStoragePlan,
    pipeline_cache: &NativeVulkanScenePipelineCachePlan,
    draw_count: usize,
) -> &'static str {
    if resource_storage.descriptor_heap.resource_descriptor_count == 0
        && pipeline_cache.pipeline_count == 0
    {
        "scene-render-graph-empty"
    } else if draw_count == 0 {
        "scene-render-graph-ready-without-mesh-draws"
    } else {
        "scene-render-graph-ready-for-vulkan-recording"
    }
}

#[cfg(test)]
mod tests {
    use super::super::pipeline_cache::NativeVulkanScenePipelineCachePlan;
    use super::super::resource_storage::{
        NativeVulkanSceneHeapStoragePlan, NativeVulkanSceneMeshBufferPlan,
        NativeVulkanSceneResourceStoragePlan,
    };
    use super::*;
    use crate::engine::scene::{
        SceneRenderPassKind, SceneRenderTargetKind, SceneRenderingDeviceGraphPlan,
        SceneRenderingDeviceMeshDraw, SceneRenderingDevicePassNode,
    };

    #[test]
    fn executor_orders_heap_pipeline_and_mesh_draw_commands() {
        let graph = SceneRenderingDeviceGraphPlan {
            pass_nodes: vec![SceneRenderingDevicePassNode {
                graph_index: 0,
                pass_record_index: 0,
                pass_id: 1,
                role: SceneRenderPassKind::BaseMaterial,
                target: SceneRenderTargetKind::SceneColor,
                binding_start: 0,
                binding_count: 0,
                mesh_draw_start: 0,
                mesh_draw_count: 1,
            }],
            mesh_draws: vec![SceneRenderingDeviceMeshDraw {
                mesh_index: 0,
                object: crate::engine::scene::SceneObjectHandle(0),
                material: crate::engine::scene::SceneMaterialHandle(
                    crate::engine::scene::INVALID_MATERIAL_ID,
                ),
                vertex_start: 0,
                vertex_count: 4,
                index_start: 0,
                index_count: 6,
            }],
            resolved_object_count: 1,
            resolved_visible_object_count: 1,
            resolved_attachment_link_count: 0,
            descriptor_heap_required: true,
            descriptor_heap_resource_count: 1,
            descriptor_heap_sampled_image_count: 0,
            descriptor_heap_uniform_buffer_count: 1,
            descriptor_heap_storage_buffer_count: 0,
            descriptor_heap_sampler_count: 0,
            fifo_latest_ready_present_required: true,
        };
        let resource_storage = NativeVulkanSceneResourceStoragePlan {
            resource_record_count: 0,
            texture_record_count: 0,
            material_record_count: 0,
            effect_record_count: 0,
            resource_payload_bytes: 0,
            mesh_buffer: NativeVulkanSceneMeshBufferPlan {
                mesh_count: 1,
                vertex_count: 4,
                index_count: 6,
                vertex_buffer_bytes: 80,
                index_buffer_bytes: 24,
                draw_count: 1,
                device_address_required: true,
            },
            descriptor_heap: NativeVulkanSceneHeapStoragePlan {
                descriptor_model: "VK_EXT_descriptor_heap",
                resource_descriptor_count: 1,
                sampled_image_descriptor_count: 0,
                uniform_buffer_descriptor_count: 1,
                storage_buffer_descriptor_count: 0,
                sampler_descriptor_count: 0,
                shader_contract_count: 1,
            },
            shader_heap_slices: Vec::new(),
            payload_residency: "scene-resource-payload-offset-slices",
            mesh_residency: "device-addressable-scene-mesh-buffers",
        };
        let pipeline_cache = NativeVulkanScenePipelineCachePlan {
            pipeline_count: 1,
            entries: Vec::new(),
            shader_catalog_entry_count: 1,
            shader_catalog_hit_count: 1,
            missing_shader_keys: Vec::new(),
            cache_model: "pipeline-key-hash-cache",
            shader_catalog_source: "built-in-scene-shader-catalog",
        };

        let plan = native_vulkan_scene_render_graph_executor_plan(
            &graph,
            &resource_storage,
            &pipeline_cache,
        );

        assert_eq!(plan.draw_count, 1);
        assert_eq!(plan.command_count, 5);
        assert_eq!(
            plan.commands[1].kind,
            NativeVulkanSceneRenderGraphCommandKind::BindDescriptorHeap
        );
        assert_eq!(
            plan.commands[3].kind,
            NativeVulkanSceneRenderGraphCommandKind::DrawMesh
        );
        assert_eq!(
            plan.executor_status,
            "scene-render-graph-ready-for-vulkan-recording"
        );
    }
}
