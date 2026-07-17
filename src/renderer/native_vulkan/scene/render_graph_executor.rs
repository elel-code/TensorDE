//! Native Vulkan scene render graph executor planning.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device_graph.*`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.*`

use serde::Serialize;

use crate::engine::scene::{
    SceneRenderPassKind, SceneRenderTargetKind, SceneRenderingDeviceDrawPrimitive,
    SceneRenderingDeviceGraphPlan, SceneRenderingDevicePassNode, SceneStringId,
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
    pub target_name: SceneStringId,
    pub binding_start: u32,
    pub binding_count: u32,
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
    DrawFullscreenTriangle,
    DrawObjectUvSupportQuad,
    CopyTarget,
    SwapTargetReferences,
    EndTarget,
    RestoreTarget,
}

pub fn native_vulkan_scene_render_graph_executor_plan(
    graph: &SceneRenderingDeviceGraphPlan,
    resource_storage: &NativeVulkanSceneResourceStoragePlan,
    pipeline_cache: &NativeVulkanScenePipelineCachePlan,
) -> NativeVulkanSceneRenderGraphExecutorPlan {
    let mut commands = Vec::new();
    for (index, pass) in graph.pass_nodes.iter().enumerate() {
        if pass.role == SceneRenderPassKind::CopyTarget {
            commands.push(command(
                index,
                NativeVulkanSceneRenderGraphCommandKind::CopyTarget,
                pass,
            ));
            continue;
        }
        if pass.role == SceneRenderPassKind::SwapTargetReferences {
            commands.push(command(
                index,
                NativeVulkanSceneRenderGraphCommandKind::SwapTargetReferences,
                pass,
            ));
            continue;
        }
        commands.push(command(
            index,
            NativeVulkanSceneRenderGraphCommandKind::BeginTarget,
            pass,
        ));
        commands.push(command(
            index,
            NativeVulkanSceneRenderGraphCommandKind::BindDescriptorHeap,
            pass,
        ));
        commands.push(command(
            index,
            NativeVulkanSceneRenderGraphCommandKind::BindPipeline,
            pass,
        ));
        if let Some(kind) = draw_command_kind(graph, pass) {
            commands.push(command(index, kind, pass));
        }
        commands.push(command(
            index,
            NativeVulkanSceneRenderGraphCommandKind::EndTarget,
            pass,
        ));
        if target_requires_restore(pass.target) {
            commands.push(command(
                index,
                NativeVulkanSceneRenderGraphCommandKind::RestoreTarget,
                pass,
            ));
        }
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
    pass: &SceneRenderingDevicePassNode,
) -> NativeVulkanSceneRenderGraphCommand {
    NativeVulkanSceneRenderGraphCommand {
        pass_node_index: pass_node_index as u32,
        kind,
        pass_role: pass.role,
        target: pass.target,
        target_name: pass.target_name,
        binding_start: pass.binding_start,
        binding_count: pass.binding_count,
        mesh_draw_start: pass.mesh_draw_start,
        mesh_draw_count: pass.mesh_draw_count,
    }
}

fn draw_command_kind(
    graph: &SceneRenderingDeviceGraphPlan,
    pass: &SceneRenderingDevicePassNode,
) -> Option<NativeVulkanSceneRenderGraphCommandKind> {
    if pass.mesh_draw_count == 0 {
        return None;
    }
    let start = pass.mesh_draw_start as usize;
    let end = start.saturating_add(pass.mesh_draw_count as usize);
    let has_fullscreen_utility = graph
        .mesh_draws
        .get(start..end)
        .unwrap_or(&[])
        .iter()
        .any(|draw| draw.primitive == SceneRenderingDeviceDrawPrimitive::FullscreenTriangle);
    let has_object_uv_support_quad = graph
        .mesh_draws
        .get(start..end)
        .unwrap_or(&[])
        .iter()
        .any(|draw| draw.primitive == SceneRenderingDeviceDrawPrimitive::ObjectUvSupportQuad);
    Some(if has_object_uv_support_quad {
        NativeVulkanSceneRenderGraphCommandKind::DrawObjectUvSupportQuad
    } else if has_fullscreen_utility {
        NativeVulkanSceneRenderGraphCommandKind::DrawFullscreenTriangle
    } else {
        NativeVulkanSceneRenderGraphCommandKind::DrawMesh
    })
}

fn target_requires_restore(target: SceneRenderTargetKind) -> bool {
    !matches!(
        target,
        SceneRenderTargetKind::SceneColor | SceneRenderTargetKind::Swapchain
    )
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
        NativeVulkanSceneEffectTargetStoragePlan, NativeVulkanSceneHeapStoragePlan,
        NativeVulkanSceneMeshBufferPlan, NativeVulkanSceneResourceStoragePlan,
        NativeVulkanSceneSkinningBufferPlan,
    };
    use super::*;
    use crate::engine::scene::{
        SceneRenderPassKind, SceneRenderTargetKind, SceneRenderingDeviceDrawPrimitive,
        SceneRenderingDeviceGraphPlan, SceneRenderingDeviceMeshDraw, SceneRenderingDevicePassNode,
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
                target_name: crate::engine::scene::SceneStringId::NONE,
                binding_start: 0,
                binding_count: 0,
                mesh_draw_start: 0,
                mesh_draw_count: 1,
            }],
            target_allocations: Vec::new(),
            effect_batches: Vec::new(),
            effect_batch_instances: Vec::new(),
            sampled_bindings: Vec::new(),
            material_sampled_bindings: Vec::new(),
            mesh_draws: vec![SceneRenderingDeviceMeshDraw {
                primitive: SceneRenderingDeviceDrawPrimitive::ObjectMesh,
                shader_key: crate::engine::scene::SceneStringId::NONE,
                mesh_index: 0,
                resolved_object_index: 0,
                clip_transform: identity_clip_transform(),
                authored_source_extent: [0.0; 2],
                skinning_palette_start: crate::engine::scene::INVALID_OBJECT_ID,
                skinning_palette_count: 0,
                resolved_color: crate::engine::scene::SceneVec3 {
                    x: 1.0,
                    y: 1.0,
                    z: 1.0,
                },
                resolved_alpha: 1.0,
                apply_resolved_visual: true,
                effect_batch_atlas_tile: u32::MAX,
                effect_batch_atlas_grid: [0; 2],
                object: crate::engine::scene::SceneObjectHandle(0),
                material: crate::engine::scene::SceneMaterialHandle(
                    crate::engine::scene::INVALID_MATERIAL_ID,
                ),
                vertex_start: 0,
                vertex_count: 4,
                index_start: 0,
                index_count: 6,
                instance_count: 1,
            }],
            puppet_bone_palettes: Vec::new(),
            puppet_bone_matrices: Vec::new(),
            particle_gpu_emitters: Vec::new(),
            resolved_object_count: 1,
            resolved_visible_object_count: 1,
            resolved_attachment_link_count: 0,
            resolved_visible_effect_instance_count: 0,
            resolved_visible_effect_pass_count: 0,
            resolved_visible_effect_fbo_count: 0,
            descriptor_heap_required: true,
            descriptor_heap_resource_count: 1,
            descriptor_heap_sampled_image_count: 0,
            descriptor_heap_uniform_buffer_count: 1,
            descriptor_heap_storage_buffer_count: 0,
            descriptor_heap_sampler_count: 0,
            graph_physical_target_count: 0,
            graph_aliased_target_count: 0,
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
            skinning_buffer: NativeVulkanSceneSkinningBufferPlan {
                palette_count: 0,
                bone_matrix_count: 0,
                bone_matrix_buffer_bytes: 0,
                storage_buffer_required: false,
            },
            effect_target_storage: NativeVulkanSceneEffectTargetStoragePlan {
                logical_target_count: 0,
                physical_target_count: 0,
                aliased_target_count: 0,
                named_fbo_count: 0,
                first_class_effect_target_count: 0,
                dynamic_rendering_image_required: false,
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

    #[test]
    fn executor_names_fullscreen_utility_draw_commands() {
        let graph = SceneRenderingDeviceGraphPlan {
            pass_nodes: vec![SceneRenderingDevicePassNode {
                graph_index: 0,
                pass_record_index: 0,
                pass_id: 1,
                role: SceneRenderPassKind::EffectMaterial,
                target: SceneRenderTargetKind::NamedFbo,
                target_name: crate::engine::scene::SceneStringId(0),
                binding_start: 0,
                binding_count: 0,
                mesh_draw_start: 0,
                mesh_draw_count: 1,
            }],
            target_allocations: Vec::new(),
            effect_batches: Vec::new(),
            effect_batch_instances: Vec::new(),
            sampled_bindings: Vec::new(),
            material_sampled_bindings: Vec::new(),
            mesh_draws: vec![SceneRenderingDeviceMeshDraw {
                primitive: SceneRenderingDeviceDrawPrimitive::FullscreenTriangle,
                shader_key: crate::engine::scene::SceneStringId::NONE,
                mesh_index: crate::engine::scene::INVALID_OBJECT_ID,
                resolved_object_index: crate::engine::scene::INVALID_OBJECT_ID,
                clip_transform: identity_clip_transform(),
                authored_source_extent: [0.0; 2],
                skinning_palette_start: crate::engine::scene::INVALID_OBJECT_ID,
                skinning_palette_count: 0,
                resolved_color: crate::engine::scene::SceneVec3 {
                    x: 1.0,
                    y: 1.0,
                    z: 1.0,
                },
                resolved_alpha: 1.0,
                apply_resolved_visual: true,
                effect_batch_atlas_tile: u32::MAX,
                effect_batch_atlas_grid: [0; 2],
                object: crate::engine::scene::SceneObjectHandle(
                    crate::engine::scene::INVALID_OBJECT_ID,
                ),
                material: crate::engine::scene::SceneMaterialHandle(
                    crate::engine::scene::INVALID_MATERIAL_ID,
                ),
                vertex_start: 0,
                vertex_count: 3,
                index_start: 0,
                index_count: 3,
                instance_count: 1,
            }],
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
            descriptor_heap_resource_count: 1,
            descriptor_heap_sampled_image_count: 0,
            descriptor_heap_uniform_buffer_count: 1,
            descriptor_heap_storage_buffer_count: 0,
            descriptor_heap_sampler_count: 0,
            graph_physical_target_count: 1,
            graph_aliased_target_count: 0,
            fifo_latest_ready_present_required: true,
        };

        let plan = native_vulkan_scene_render_graph_executor_plan(
            &graph,
            &empty_resource_storage(),
            &pipeline_cache_with_count(1),
        );

        assert!(plan.commands.iter().any(|command| {
            command.kind == NativeVulkanSceneRenderGraphCommandKind::DrawFullscreenTriangle
        }));
    }

    #[test]
    fn executor_commandizes_copy_swap_and_target_restore_passes() {
        let graph = SceneRenderingDeviceGraphPlan {
            pass_nodes: vec![
                SceneRenderingDevicePassNode {
                    graph_index: 0,
                    pass_record_index: 0,
                    pass_id: 1,
                    role: SceneRenderPassKind::CopyTarget,
                    target: SceneRenderTargetKind::NamedFbo,
                    target_name: crate::engine::scene::SceneStringId(0),
                    binding_start: 0,
                    binding_count: 1,
                    mesh_draw_start: 0,
                    mesh_draw_count: 0,
                },
                SceneRenderingDevicePassNode {
                    graph_index: 0,
                    pass_record_index: 1,
                    pass_id: 2,
                    role: SceneRenderPassKind::SwapTargetReferences,
                    target: SceneRenderTargetKind::NamedFbo,
                    target_name: crate::engine::scene::SceneStringId(1),
                    binding_start: 1,
                    binding_count: 1,
                    mesh_draw_start: 0,
                    mesh_draw_count: 0,
                },
                SceneRenderingDevicePassNode {
                    graph_index: 0,
                    pass_record_index: 2,
                    pass_id: 3,
                    role: SceneRenderPassKind::EffectMaterial,
                    target: SceneRenderTargetKind::FirstClassEffectTarget,
                    target_name: crate::engine::scene::SceneStringId(2),
                    binding_start: 2,
                    binding_count: 1,
                    mesh_draw_start: 0,
                    mesh_draw_count: 0,
                },
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
            descriptor_heap_resource_count: 1,
            descriptor_heap_sampled_image_count: 0,
            descriptor_heap_uniform_buffer_count: 1,
            descriptor_heap_storage_buffer_count: 0,
            descriptor_heap_sampler_count: 0,
            graph_physical_target_count: 1,
            graph_aliased_target_count: 0,
            fifo_latest_ready_present_required: true,
        };

        let plan = native_vulkan_scene_render_graph_executor_plan(
            &graph,
            &empty_resource_storage(),
            &pipeline_cache_with_count(1),
        );

        assert_eq!(
            plan.commands[0].kind,
            NativeVulkanSceneRenderGraphCommandKind::CopyTarget
        );
        assert_eq!(plan.commands[0].binding_start, 0);
        assert_eq!(
            plan.commands[1].kind,
            NativeVulkanSceneRenderGraphCommandKind::SwapTargetReferences
        );
        assert!(
            plan.commands
                .iter()
                .any(|command| command.kind
                    == NativeVulkanSceneRenderGraphCommandKind::RestoreTarget)
        );
    }

    fn empty_resource_storage() -> NativeVulkanSceneResourceStoragePlan {
        NativeVulkanSceneResourceStoragePlan {
            resource_record_count: 0,
            texture_record_count: 0,
            material_record_count: 0,
            effect_record_count: 0,
            resource_payload_bytes: 0,
            mesh_buffer: NativeVulkanSceneMeshBufferPlan {
                mesh_count: 0,
                vertex_count: 0,
                index_count: 0,
                vertex_buffer_bytes: 0,
                index_buffer_bytes: 0,
                draw_count: 0,
                device_address_required: false,
            },
            skinning_buffer: NativeVulkanSceneSkinningBufferPlan {
                palette_count: 0,
                bone_matrix_count: 0,
                bone_matrix_buffer_bytes: 0,
                storage_buffer_required: false,
            },
            effect_target_storage: NativeVulkanSceneEffectTargetStoragePlan {
                logical_target_count: 0,
                physical_target_count: 0,
                aliased_target_count: 0,
                named_fbo_count: 0,
                first_class_effect_target_count: 0,
                dynamic_rendering_image_required: false,
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
        }
    }

    fn pipeline_cache_with_count(pipeline_count: usize) -> NativeVulkanScenePipelineCachePlan {
        NativeVulkanScenePipelineCachePlan {
            pipeline_count,
            entries: Vec::new(),
            shader_catalog_entry_count: 1,
            shader_catalog_hit_count: pipeline_count,
            missing_shader_keys: Vec::new(),
            cache_model: "pipeline-key-hash-cache",
            shader_catalog_source: "built-in-scene-shader-catalog",
        }
    }

    fn identity_clip_transform() -> [[f32; 4]; 4] {
        [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ]
    }
}
