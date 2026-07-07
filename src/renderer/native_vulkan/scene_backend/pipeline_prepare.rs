//! Scene mesh pipeline cold-path prepare boundary.
//!
//! References:
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/renderer_rd/pipeline_hash_map_rd.h`
//! - `references/godot/servers/rendering/rendering_device.h`
//! - `references/godot/drivers/vulkan/rendering_device_driver_vulkan.cpp`

use serde::Serialize;
use vulkanalia::prelude::v1_4::*;
use vulkanalia::vk;

use crate::engine::scene_engine::SceneGraph;

use super::frame_resources::NativeVulkanSceneFrameResources;
use super::pipeline_factory::{
    NativeVulkanSceneMeshPipelineLayoutSpec, NativeVulkanSceneMeshPipelineShaders,
};
use super::pipeline_warmup::NativeVulkanSceneMeshPipelineWarmupPlan;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMeshPipelinePreparePlan {
    pub target_format: String,
    pub draw_count: usize,
    pub cache_key_count: usize,
    pub created_pipeline_count: usize,
    pub reused_pipeline_count: usize,
    pub resource_descriptor_count: usize,
    pub sampler_descriptor_count: usize,
    pub descriptor_model: &'static str,
    pub command_order: [&'static str; 4],
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_prepare_scene_mesh_pipeline_cache(
    device: &Device,
    frame_resources: &mut NativeVulkanSceneFrameResources,
    graph: &SceneGraph,
    target_format: vk::Format,
    shaders: NativeVulkanSceneMeshPipelineShaders<'_>,
) -> Result<NativeVulkanSceneMeshPipelinePreparePlan, String> {
    let warmup =
        NativeVulkanSceneMeshPipelineWarmupPlan::from_swapchain_graph(graph, target_format)?;
    let resource_heap = frame_resources
        .current_resource_heap_frame_plan()
        .ok_or_else(|| {
            "scene mesh pipeline prepare requires current draw resource heap frame plan".to_owned()
        })?;
    let descriptor_heap_plan = resource_heap.descriptor_heap_plan.clone();
    let resource_descriptor_count = resource_heap.resource_descriptor_count;
    let sampler_descriptor_count = resource_heap.sampler_descriptor_count;
    let descriptor_model = resource_heap.descriptor_model;
    let pipeline_layout = NativeVulkanSceneMeshPipelineLayoutSpec {
        draw_resource_heap_plan: &descriptor_heap_plan,
    };
    let mut created_pipeline_count = 0usize;
    let mut reused_pipeline_count = 0usize;

    for key in warmup.cache_keys().iter().cloned() {
        if frame_resources.has_mesh_pipeline(&key) {
            reused_pipeline_count = reused_pipeline_count.saturating_add(1);
        } else {
            created_pipeline_count = created_pipeline_count.saturating_add(1);
        }
        frame_resources.resolve_mesh_pipeline(device, key, shaders, pipeline_layout)?;
    }

    NativeVulkanSceneMeshPipelinePreparePlan::from_counts(
        &warmup,
        created_pipeline_count,
        reused_pipeline_count,
        resource_descriptor_count,
        sampler_descriptor_count,
        descriptor_model,
    )
}

impl NativeVulkanSceneMeshPipelinePreparePlan {
    fn from_counts(
        warmup: &NativeVulkanSceneMeshPipelineWarmupPlan,
        created_pipeline_count: usize,
        reused_pipeline_count: usize,
        resource_descriptor_count: usize,
        sampler_descriptor_count: usize,
        descriptor_model: &'static str,
    ) -> Result<Self, String> {
        let cache_key_count = warmup.cache_keys().len();
        if created_pipeline_count.saturating_add(reused_pipeline_count) != cache_key_count {
            return Err(format!(
                "scene mesh pipeline prepare counted {} create/reuse actions for {} cache keys",
                created_pipeline_count.saturating_add(reused_pipeline_count),
                cache_key_count
            ));
        }
        Ok(Self {
            target_format: format!("{:?}", warmup.target_format()),
            draw_count: warmup.draw_count(),
            cache_key_count,
            created_pipeline_count,
            reused_pipeline_count,
            resource_descriptor_count,
            sampler_descriptor_count,
            descriptor_model,
            command_order: [
                "collect_unique_pipeline_keys",
                "read_current_draw_resource_heap_plan",
                "resolve_scene_mesh_pipeline_cache",
                "require_warmed_pipeline_cache_for_present_frame",
            ],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneGeometryId, SceneGraphDraw, SceneGraphPass,
        SceneGraphPipelineClass, SceneGraphResourceBinding, SceneGraphResourceRole,
        SceneGraphTarget, SceneMaterialKey, SceneObjectId, SceneResourceId,
    };

    #[test]
    fn pipeline_prepare_plan_records_cold_path_create_and_reuse_counts() {
        let graph = mesh_graph(vec![
            mesh_draw(SceneObjectId(1), SceneBlendContract::TranslucentAlpha),
            mesh_draw(SceneObjectId(2), SceneBlendContract::Additive),
        ]);
        let warmup = NativeVulkanSceneMeshPipelineWarmupPlan::from_swapchain_graph(
            &graph,
            vk::Format::B8G8R8A8_UNORM,
        )
        .expect("warmup");

        let plan = NativeVulkanSceneMeshPipelinePreparePlan::from_counts(
            &warmup,
            1,
            1,
            4,
            2,
            "VK_EXT_descriptor_heap",
        )
        .expect("prepare plan");

        assert_eq!(plan.target_format, "B8G8R8A8_UNORM");
        assert_eq!(plan.draw_count, 2);
        assert_eq!(plan.cache_key_count, 2);
        assert_eq!(plan.created_pipeline_count, 1);
        assert_eq!(plan.reused_pipeline_count, 1);
        assert_eq!(plan.resource_descriptor_count, 4);
        assert_eq!(plan.sampler_descriptor_count, 2);
        assert_eq!(plan.descriptor_model, "VK_EXT_descriptor_heap");
        assert_eq!(
            plan.command_order,
            [
                "collect_unique_pipeline_keys",
                "read_current_draw_resource_heap_plan",
                "resolve_scene_mesh_pipeline_cache",
                "require_warmed_pipeline_cache_for_present_frame"
            ]
        );
    }

    #[test]
    fn pipeline_prepare_plan_rejects_action_count_mismatch() {
        let graph = mesh_graph(vec![mesh_draw(
            SceneObjectId(1),
            SceneBlendContract::TranslucentAlpha,
        )]);
        let warmup = NativeVulkanSceneMeshPipelineWarmupPlan::from_swapchain_graph(
            &graph,
            vk::Format::B8G8R8A8_UNORM,
        )
        .expect("warmup");

        let err = NativeVulkanSceneMeshPipelinePreparePlan::from_counts(
            &warmup,
            0,
            0,
            2,
            1,
            "VK_EXT_descriptor_heap",
        )
        .expect_err("mismatch must fail");

        assert!(err.contains("create/reuse actions"));
    }

    fn mesh_graph(draws: Vec<SceneGraphDraw>) -> SceneGraph {
        SceneGraph {
            passes: vec![SceneGraphPass {
                name: "scene-main".to_owned(),
                input: None,
                output: SceneGraphTarget::Swapchain,
                draws,
            }],
        }
    }

    fn mesh_draw(object: SceneObjectId, blend: SceneBlendContract) -> SceneGraphDraw {
        SceneGraphDraw {
            object,
            pipeline: SceneGraphPipelineClass::Mesh,
            material: SceneMaterialKey {
                shader: "we/genericimage4".to_owned(),
                blend,
                render_state: crate::engine::scene_engine::SceneMaterialRenderState::translucent_2d(
                ),
            },
            geometry: Some(SceneGeometryId(object.0)),
            puppet: None,
            resources: vec![SceneGraphResourceBinding {
                slot: 0,
                role: SceneGraphResourceRole::shader_texture(0),
                resource: SceneResourceId(object.0),
            }],
            index_count: 6,
        }
    }
}
