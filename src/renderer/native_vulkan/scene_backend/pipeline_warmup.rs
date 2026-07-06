//! Scene mesh pipeline warmup planning.
//!
//! References:
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `references/godot/servers/rendering/renderer_rd/pipeline_hash_map_rd.h`
//! - `references/godot/servers/rendering/renderer_rd/renderer_canvas_render_rd.h`
//! - `references/godot/servers/rendering/rendering_device.h`

use vulkanalia::vk;

use crate::engine::scene_engine::{SceneGraph, SceneGraphPipelineClass, SceneGraphTarget};

use super::pipeline::{NativeVulkanScenePipelineCacheKey, NativeVulkanScenePipelineKey};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMeshPipelineWarmupPlan {
    target_format: vk::Format,
    draw_count: usize,
    cache_keys: Vec<NativeVulkanScenePipelineCacheKey>,
    command_order: [&'static str; 2],
}

impl NativeVulkanSceneMeshPipelineWarmupPlan {
    pub(in crate::renderer::native_vulkan) fn from_swapchain_graph(
        graph: &SceneGraph,
        target_format: vk::Format,
    ) -> Result<Self, String> {
        if target_format == vk::Format::UNDEFINED {
            return Err("scene mesh pipeline warmup requires defined target format".to_owned());
        }

        let mut cache_keys = Vec::new();
        let mut draw_count = 0usize;
        for pass in &graph.passes {
            if pass.output != SceneGraphTarget::Swapchain {
                return Err(format!(
                    "scene mesh pipeline warmup requires swapchain output until offscreen target formats are explicit, got {:?}",
                    pass.output
                ));
            }

            for draw in &pass.draws {
                if draw.pipeline != SceneGraphPipelineClass::Mesh {
                    return Err(format!(
                        "scene mesh pipeline warmup requires Mesh pipeline, got {:?} for object {:?}",
                        draw.pipeline, draw.object
                    ));
                }

                draw_count += 1;
                let key = NativeVulkanScenePipelineCacheKey::from_bind_key(
                    NativeVulkanScenePipelineKey::from_draw(draw)?,
                    target_format,
                )?;
                if !cache_keys.iter().any(|existing| existing == &key) {
                    cache_keys.push(key);
                }
            }
        }

        Ok(Self {
            target_format,
            draw_count,
            cache_keys,
            command_order: ["collect_unique_pipeline_keys", "resolve_pipeline_cache"],
        })
    }

    pub(in crate::renderer::native_vulkan) fn target_format(&self) -> vk::Format {
        self.target_format
    }

    pub(in crate::renderer::native_vulkan) fn draw_count(&self) -> usize {
        self.draw_count
    }

    pub(in crate::renderer::native_vulkan) fn cache_keys(
        &self,
    ) -> &[NativeVulkanScenePipelineCacheKey] {
        &self.cache_keys
    }

    pub(in crate::renderer::native_vulkan) fn command_order(&self) -> [&'static str; 2] {
        self.command_order
    }
}

#[cfg(test)]
mod tests {
    use super::super::pipeline::NativeVulkanScenePipelineVertexLayout;
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneGeometryId, SceneGraphDraw, SceneGraphPass, SceneMaterialKey,
        SceneObjectId,
    };

    #[test]
    fn warmup_plan_collects_unique_mesh_pipeline_keys_in_first_use_order() {
        let graph = mesh_graph(vec![
            mesh_draw(
                SceneObjectId(1),
                "we/genericimage4",
                SceneBlendContract::TranslucentAlpha,
            ),
            mesh_draw(
                SceneObjectId(2),
                "we/genericimage4",
                SceneBlendContract::TranslucentAlpha,
            ),
            mesh_draw(
                SceneObjectId(3),
                "we/additive",
                SceneBlendContract::Additive,
            ),
            mesh_draw(
                SceneObjectId(4),
                "we/genericimage4",
                SceneBlendContract::TranslucentAlpha,
            ),
        ]);

        let plan = NativeVulkanSceneMeshPipelineWarmupPlan::from_swapchain_graph(
            &graph,
            vk::Format::B8G8R8A8_UNORM,
        )
        .expect("mesh pipeline warmup");

        assert_eq!(plan.target_format(), vk::Format::B8G8R8A8_UNORM);
        assert_eq!(plan.draw_count(), 4);
        assert_eq!(plan.cache_keys().len(), 2);
        assert_eq!(plan.cache_keys()[0].shader, "we/genericimage4");
        assert_eq!(plan.cache_keys()[1].shader, "we/additive");
        assert_eq!(
            plan.cache_keys()[0].blend,
            SceneBlendContract::TranslucentAlpha
        );
        assert_eq!(plan.cache_keys()[1].blend, SceneBlendContract::Additive);
        assert_eq!(
            plan.cache_keys()[0].vertex_layout,
            NativeVulkanScenePipelineVertexLayout::SceneMeshV0
        );
        assert_eq!(
            plan.command_order(),
            ["collect_unique_pipeline_keys", "resolve_pipeline_cache"]
        );
    }

    #[test]
    fn warmup_plan_rejects_undefined_target_format() {
        let graph = mesh_graph(vec![mesh_draw(
            SceneObjectId(1),
            "we/genericimage4",
            SceneBlendContract::TranslucentAlpha,
        )]);

        let err = NativeVulkanSceneMeshPipelineWarmupPlan::from_swapchain_graph(
            &graph,
            vk::Format::UNDEFINED,
        )
        .expect_err("undefined target format must fail");

        assert!(err.contains("defined target format"));
    }

    #[test]
    fn warmup_plan_rejects_non_mesh_draws() {
        let mut draw = mesh_draw(
            SceneObjectId(1),
            "we/genericimage4",
            SceneBlendContract::TranslucentAlpha,
        );
        draw.pipeline = SceneGraphPipelineClass::PuppetSkinning;
        let graph = mesh_graph(vec![draw]);

        let err = NativeVulkanSceneMeshPipelineWarmupPlan::from_swapchain_graph(
            &graph,
            vk::Format::B8G8R8A8_UNORM,
        )
        .expect_err("non-mesh pipeline must fail");

        assert!(err.contains("requires Mesh pipeline"));
    }

    #[test]
    fn warmup_plan_rejects_non_swapchain_target_without_format_resolver() {
        let graph = SceneGraph {
            passes: vec![SceneGraphPass {
                name: "scene-offscreen".to_owned(),
                input: None,
                output: SceneGraphTarget::ImageLocalMain(0),
                draws: vec![mesh_draw(
                    SceneObjectId(1),
                    "we/genericimage4",
                    SceneBlendContract::TranslucentAlpha,
                )],
            }],
        };

        let err = NativeVulkanSceneMeshPipelineWarmupPlan::from_swapchain_graph(
            &graph,
            vk::Format::B8G8R8A8_UNORM,
        )
        .expect_err("offscreen target format must be explicit");

        assert!(err.contains("swapchain output"));
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

    fn mesh_draw(object: SceneObjectId, shader: &str, blend: SceneBlendContract) -> SceneGraphDraw {
        SceneGraphDraw {
            object,
            pipeline: SceneGraphPipelineClass::Mesh,
            material: SceneMaterialKey {
                shader: shader.to_owned(),
                blend,
                writes_depth: false,
                tests_depth: false,
            },
            geometry: Some(SceneGeometryId(object.0)),
            puppet: None,
            resources: Vec::new(),
            index_count: 6,
        }
    }
}
