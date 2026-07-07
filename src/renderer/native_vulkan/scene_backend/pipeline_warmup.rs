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

use crate::engine::scene_engine::{SceneGraph, SceneGraphTarget};

use super::pipeline::{NativeVulkanScenePipelineCacheKey, NativeVulkanScenePipelineKey};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMeshPipelineWarmupPlan {
    draw_count: usize,
    target_formats: Vec<vk::Format>,
    cache_keys: Vec<NativeVulkanScenePipelineCacheKey>,
    command_order: [&'static str; 4],
}

impl NativeVulkanSceneMeshPipelineWarmupPlan {
    pub(in crate::renderer::native_vulkan) fn from_swapchain_graph(
        graph: &SceneGraph,
        target_format: vk::Format,
    ) -> Result<Self, String> {
        if target_format == vk::Format::UNDEFINED {
            return Err("scene mesh pipeline warmup requires defined target format".to_owned());
        }
        Self::from_graph_with_target_formats(graph, |target| match target {
            SceneGraphTarget::Swapchain => Ok(target_format),
            target => Err(format!(
                "scene mesh pipeline warmup requires explicit format for graph target {:?}",
                target
            )),
        })
    }

    pub(in crate::renderer::native_vulkan) fn from_graph_with_target_formats<TargetFormat>(
        graph: &SceneGraph,
        target_format: TargetFormat,
    ) -> Result<Self, String>
    where
        TargetFormat: FnMut(SceneGraphTarget) -> Result<vk::Format, String>,
    {
        Self::from_graph_with_target_formats_and_extra_cache_keys(graph, target_format, &[])
    }

    pub(in crate::renderer::native_vulkan) fn from_graph_with_target_formats_and_extra_cache_keys<
        TargetFormat,
    >(
        graph: &SceneGraph,
        mut target_format: TargetFormat,
        extra_cache_keys: &[NativeVulkanScenePipelineCacheKey],
    ) -> Result<Self, String>
    where
        TargetFormat: FnMut(SceneGraphTarget) -> Result<vk::Format, String>,
    {
        let mut cache_keys = Vec::new();
        let mut target_formats = Vec::new();
        let mut draw_count = 0usize;
        for pass in &graph.passes {
            let pass_target_format = target_format(pass.output)?;
            if pass_target_format == vk::Format::UNDEFINED {
                return Err(format!(
                    "scene mesh pipeline warmup requires defined format for graph target {:?}",
                    pass.output
                ));
            }
            if !target_formats.contains(&pass_target_format) {
                target_formats.push(pass_target_format);
            }

            for draw in &pass.draws {
                if !draw.pipeline.is_indexed_mesh_graphics() {
                    return Err(format!(
                        "scene mesh pipeline warmup requires indexed mesh graphics pipeline, got {:?} for object {:?}",
                        draw.pipeline, draw.object
                    ));
                }

                draw_count += 1;
                let key = NativeVulkanScenePipelineCacheKey::from_bind_key(
                    NativeVulkanScenePipelineKey::from_draw_with_pass_input(draw, pass.input)?,
                    pass_target_format,
                )?;
                if !cache_keys.iter().any(|existing| existing == &key) {
                    cache_keys.push(key);
                }
            }
        }

        for key in extra_cache_keys {
            if key.target_format == vk::Format::UNDEFINED {
                return Err(format!(
                    "scene mesh pipeline warmup extra key for shader '{}' requires defined target format",
                    key.shader
                ));
            }
            if !target_formats.contains(&key.target_format) {
                target_formats.push(key.target_format);
            }
            if !cache_keys.iter().any(|existing| existing == key) {
                cache_keys.push(key.clone());
            }
        }

        Ok(Self {
            draw_count,
            target_formats,
            cache_keys,
            command_order: [
                "resolve_graph_target_formats",
                "collect_unique_pipeline_keys",
                "append_layer_alpha_mask_pipeline_keys",
                "require_warmed_pipeline_cache",
            ],
        })
    }

    pub(in crate::renderer::native_vulkan) fn target_format(&self) -> vk::Format {
        self.target_formats
            .first()
            .copied()
            .unwrap_or(vk::Format::UNDEFINED)
    }

    pub(in crate::renderer::native_vulkan) fn target_formats(&self) -> &[vk::Format] {
        &self.target_formats
    }

    pub(in crate::renderer::native_vulkan) fn draw_count(&self) -> usize {
        self.draw_count
    }

    pub(in crate::renderer::native_vulkan) fn cache_keys(
        &self,
    ) -> &[NativeVulkanScenePipelineCacheKey] {
        &self.cache_keys
    }

    pub(in crate::renderer::native_vulkan) fn command_order(&self) -> [&'static str; 4] {
        self.command_order
    }
}

#[cfg(test)]
mod tests {
    use super::super::pipeline::NativeVulkanScenePipelineVertexLayout;
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneGeometryId, SceneGraphDraw, SceneGraphPass,
        SceneGraphPipelineClass, SceneGraphResourceBinding, SceneGraphResourceRole,
        SceneMaterialKey, SceneObjectId,
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
                "we/genericimage4",
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
        assert_eq!(plan.cache_keys()[1].shader, "we/genericimage4");
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
            [
                "resolve_graph_target_formats",
                "collect_unique_pipeline_keys",
                "append_layer_alpha_mask_pipeline_keys",
                "require_warmed_pipeline_cache"
            ]
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
    fn warmup_plan_keeps_puppet_skinning_pipeline_keys() {
        let mut draw = mesh_draw(
            SceneObjectId(1),
            "we/genericimage4",
            SceneBlendContract::TranslucentAlpha,
        );
        draw.pipeline = SceneGraphPipelineClass::PuppetSkinning;
        draw.puppet = Some(crate::engine::scene_engine::ScenePuppetId(9));
        let graph = mesh_graph(vec![draw]);

        let plan = NativeVulkanSceneMeshPipelineWarmupPlan::from_swapchain_graph(
            &graph,
            vk::Format::B8G8R8A8_UNORM,
        )
        .expect("puppet skinning participates in indexed graphics warmup");

        assert_eq!(plan.cache_keys().len(), 1);
        assert_eq!(
            plan.cache_keys()[0].pipeline_class,
            SceneGraphPipelineClass::PuppetSkinning
        );
    }

    #[test]
    fn warmup_plan_rejects_non_indexed_graphics_draws() {
        let mut draw = mesh_draw(
            SceneObjectId(1),
            "we/genericimage4",
            SceneBlendContract::TranslucentAlpha,
        );
        draw.pipeline = SceneGraphPipelineClass::Quad;
        let graph = mesh_graph(vec![draw]);

        let err = NativeVulkanSceneMeshPipelineWarmupPlan::from_swapchain_graph(
            &graph,
            vk::Format::B8G8R8A8_UNORM,
        )
        .expect_err("quad pipeline must fail until quad executor exists");

        assert!(err.contains("requires indexed mesh graphics pipeline"));
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

        assert!(err.contains("explicit format for graph target"));
    }

    #[test]
    fn warmup_plan_accepts_offscreen_target_with_explicit_format_resolver() {
        let graph = SceneGraph {
            passes: vec![
                SceneGraphPass {
                    name: "scene-offscreen".to_owned(),
                    input: None,
                    output: SceneGraphTarget::ImageLocalMain(0),
                    draws: vec![mesh_draw(
                        SceneObjectId(1),
                        "we/genericimage4",
                        SceneBlendContract::TranslucentAlpha,
                    )],
                },
                SceneGraphPass {
                    name: "scene-main".to_owned(),
                    input: Some(SceneGraphTarget::ImageLocalMain(0)),
                    output: SceneGraphTarget::Swapchain,
                    draws: vec![mesh_draw_without_resources(
                        SceneObjectId(2),
                        "we/genericimage4",
                        SceneBlendContract::Additive,
                    )],
                },
            ],
        };

        let plan = NativeVulkanSceneMeshPipelineWarmupPlan::from_graph_with_target_formats(
            &graph,
            |target| match target {
                SceneGraphTarget::ImageLocalMain(0) => Ok(vk::Format::R16G16B16A16_SFLOAT),
                SceneGraphTarget::Swapchain => Ok(vk::Format::B8G8R8A8_UNORM),
                target => Err(format!("unexpected target {target:?}")),
            },
        )
        .expect("explicit target formats allow graph-wide pipeline warmup");

        assert_eq!(plan.draw_count(), 2);
        assert_eq!(
            plan.target_formats(),
            &[vk::Format::R16G16B16A16_SFLOAT, vk::Format::B8G8R8A8_UNORM]
        );
        assert_eq!(plan.cache_keys().len(), 2);
        assert_eq!(
            plan.cache_keys()
                .iter()
                .map(|key| key.target_format)
                .collect::<Vec<_>>(),
            vec![vk::Format::R16G16B16A16_SFLOAT, vk::Format::B8G8R8A8_UNORM]
        );
    }

    #[test]
    fn warmup_plan_appends_layer_alpha_mask_pipeline_keys() {
        let graph = mesh_graph(vec![mesh_draw(
            SceneObjectId(1),
            "we/genericimage4",
            SceneBlendContract::TranslucentAlpha,
        )]);
        let alpha_mask_key = NativeVulkanScenePipelineCacheKey {
            shader: "we/clippingmaskimage4".to_owned(),
            shader_combo_values: Vec::new(),
            blend: SceneBlendContract::TranslucentAlpha,
            render_state: crate::engine::scene_engine::SceneMaterialRenderState::translucent_2d(),
            pipeline_class: SceneGraphPipelineClass::PuppetSkinning,
            vertex_layout: NativeVulkanScenePipelineVertexLayout::SceneMeshV0,
            target_format: vk::Format::R8_UNORM,
            texture_slot_mask: (1u32 << 0) | (1u32 << 1) | (1u32 << 5),
        };

        let plan =
            NativeVulkanSceneMeshPipelineWarmupPlan::from_graph_with_target_formats_and_extra_cache_keys(
                &graph,
                |target| match target {
                    SceneGraphTarget::Swapchain => Ok(vk::Format::B8G8R8A8_UNORM),
                    target => Err(format!("unexpected target {target:?}")),
                },
                std::slice::from_ref(&alpha_mask_key),
            )
            .expect("warmup with alpha-mask pipeline");

        assert_eq!(plan.draw_count(), 1);
        assert_eq!(plan.cache_keys().len(), 2);
        assert!(plan.cache_keys().contains(&alpha_mask_key));
        assert_eq!(
            plan.target_formats(),
            &[vk::Format::B8G8R8A8_UNORM, vk::Format::R8_UNORM]
        );
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
                render_state: crate::engine::scene_engine::SceneMaterialRenderState::translucent_2d(
                ),
            },
            geometry: Some(SceneGeometryId(object.0)),
            puppet: None,
            resources: vec![SceneGraphResourceBinding {
                slot: 0,
                role: SceneGraphResourceRole::shader_texture(0),
                resource: crate::engine::scene_engine::SceneResourceId(object.0),
            }],
            index_count: 6,
        }
    }

    fn mesh_draw_without_resources(
        object: SceneObjectId,
        shader: &str,
        blend: SceneBlendContract,
    ) -> SceneGraphDraw {
        let mut draw = mesh_draw(object, shader, blend);
        draw.resources.clear();
        draw
    }
}
