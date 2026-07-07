//! Vulkan scene draw-family executor dispatch.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`

use serde::Serialize;

use crate::engine::scene_engine::SceneGraphDrawFamilyPlan;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneDrawFamilyExecutorPlan {
    pub draw_count: usize,
    pub indexed_mesh_graphics_draw_count: usize,
    pub quad_draw_count: usize,
    pub layer_utility_indexed_draw_count: usize,
    pub particle_emitter_draw_count: usize,
    pub missing_executor_draw_count: usize,
    pub executor_model: &'static str,
    pub command_order: [&'static str; 3],
}

impl NativeVulkanSceneDrawFamilyExecutorPlan {
    pub(in crate::renderer::native_vulkan) fn from_family_plan(
        plan: &SceneGraphDrawFamilyPlan,
    ) -> Self {
        Self {
            draw_count: plan.draw_count,
            indexed_mesh_graphics_draw_count: plan.indexed_mesh_graphics_draw_count,
            quad_draw_count: plan.quad_draw_count,
            layer_utility_indexed_draw_count: plan.layer_utility_indexed_draw_count,
            particle_emitter_draw_count: plan.particle_emitter_draw_count,
            missing_executor_draw_count: plan.unsupported_runtime_draw_count(),
            executor_model: "godot_style_family_dispatch",
            command_order: [
                "classify_scene_graph_draw_families",
                "dispatch_indexed_mesh_graphics_executor",
                "require_explicit_non_mesh_family_executor",
            ],
        }
    }
}

pub(in crate::renderer::native_vulkan) fn native_vulkan_require_scene_mesh_executor_families(
    plan: &SceneGraphDrawFamilyPlan,
) -> Result<NativeVulkanSceneDrawFamilyExecutorPlan, String> {
    let executor = NativeVulkanSceneDrawFamilyExecutorPlan::from_family_plan(plan);
    if executor.missing_executor_draw_count == 0 {
        return Ok(executor);
    }

    Err(format!(
        "scene graph contains draw families without a native Vulkan executor: quad={}, layer_utility_indexed={}, particle_emitter={}; refusing to downgrade them into the indexed mesh executor",
        executor.quad_draw_count,
        executor.layer_utility_indexed_draw_count,
        executor.particle_emitter_draw_count
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneGraph, SceneGraphDraw, SceneGraphPass, SceneGraphPipelineClass,
        SceneGraphTarget, SceneMaterialKey, SceneObjectId,
    };

    #[test]
    fn executor_plan_accepts_indexed_mesh_family() {
        let graph = graph(vec![draw(
            SceneObjectId(1),
            SceneGraphPipelineClass::PuppetSkinning,
        )]);
        let family = SceneGraphDrawFamilyPlan::from_graph(&graph);

        let plan = native_vulkan_require_scene_mesh_executor_families(&family)
            .expect("indexed graphics family has a mesh executor");

        assert_eq!(plan.indexed_mesh_graphics_draw_count, 1);
        assert_eq!(plan.missing_executor_draw_count, 0);
        assert_eq!(plan.executor_model, "godot_style_family_dispatch");
    }

    #[test]
    fn executor_plan_rejects_non_mesh_family_without_downgrade() {
        let graph = graph(vec![
            draw(SceneObjectId(1), SceneGraphPipelineClass::Mesh),
            draw(SceneObjectId(2), SceneGraphPipelineClass::Quad),
            draw(
                SceneObjectId(3),
                SceneGraphPipelineClass::LayerUtilityIndexed,
            ),
            draw(SceneObjectId(4), SceneGraphPipelineClass::ParticleEmitter),
        ]);
        let family = SceneGraphDrawFamilyPlan::from_graph(&graph);

        let err = native_vulkan_require_scene_mesh_executor_families(&family)
            .expect_err("quad and particle need explicit executors");

        assert!(err.contains("quad=1"));
        assert!(err.contains("layer_utility_indexed=1"));
        assert!(err.contains("particle_emitter=1"));
        assert!(err.contains("refusing to downgrade"));
    }

    fn graph(draws: Vec<SceneGraphDraw>) -> SceneGraph {
        SceneGraph {
            passes: vec![SceneGraphPass {
                name: "scene-main".to_owned(),
                input: None,
                output: SceneGraphTarget::Swapchain,
                draws,
            }],
        }
    }

    fn draw(object: SceneObjectId, pipeline: SceneGraphPipelineClass) -> SceneGraphDraw {
        SceneGraphDraw {
            object,
            pipeline,
            material: SceneMaterialKey {
                shader: "we/genericimage4".to_owned(),
                blend: SceneBlendContract::TranslucentAlpha,
                render_state: crate::engine::scene_engine::SceneMaterialRenderState::translucent_2d(
                ),
            },
            geometry: pipeline
                .is_indexed_mesh_graphics()
                .then_some(crate::engine::scene_engine::SceneGeometryId(object.0)),
            puppet: None,
            resources: Vec::new(),
            index_count: 6,
        }
    }
}
