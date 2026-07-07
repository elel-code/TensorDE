//! Scene mesh draw-list state tracking.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/renderer_rd/forward_clustered/render_forward_clustered.h`
//! - `references/godot/servers/rendering/renderer_rd/forward_mobile/render_forward_mobile.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`

use crate::engine::scene_engine::{SceneGraphDraw, SceneGraphTarget};

use super::pipeline::NativeVulkanScenePipelineKey;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMeshDrawListTransition<'a> {
    pub pipeline_key: NativeVulkanScenePipelineKey<'a>,
    pub bind_pipeline: bool,
}

#[derive(Debug, Default)]
pub(in crate::renderer::native_vulkan) struct NativeVulkanSceneMeshDrawListState<'a> {
    last_pipeline_key: Option<NativeVulkanScenePipelineKey<'a>>,
}

impl<'a> NativeVulkanSceneMeshDrawListState<'a> {
    pub fn next_draw(
        &mut self,
        pass_name: &str,
        pass_input: Option<SceneGraphTarget>,
        draw: &'a SceneGraphDraw,
    ) -> Result<NativeVulkanSceneMeshDrawListTransition<'a>, String> {
        if !draw.pipeline.is_indexed_mesh_graphics() {
            return Err(format!(
                "scene mesh pass '{}' requires indexed mesh graphics pipeline, got {:?} for object {:?}",
                pass_name, draw.pipeline, draw.object
            ));
        }

        let pipeline_key =
            NativeVulkanScenePipelineKey::from_draw_with_pass_input(draw, pass_input)?;
        let bind_pipeline = self.last_pipeline_key != Some(pipeline_key);

        self.last_pipeline_key = Some(pipeline_key);

        Ok(NativeVulkanSceneMeshDrawListTransition {
            pipeline_key,
            bind_pipeline,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneGeometryId, SceneGraphPipelineClass, SceneGraphResourceBinding,
        SceneGraphResourceRole, SceneMaterialKey, SceneObjectId, SceneResourceId,
    };

    #[test]
    fn mesh_draw_list_keeps_we_order_but_skips_redundant_pipeline_binds() {
        let mut additive_draw = mesh_draw(
            SceneObjectId(4),
            SceneGeometryId(7),
            "we/genericimage4",
            Some(SceneResourceId(8)),
        );
        additive_draw.material.blend = SceneBlendContract::Additive;
        let draws = [
            mesh_draw(
                SceneObjectId(1),
                SceneGeometryId(4),
                "we/genericimage4",
                Some(SceneResourceId(7)),
            ),
            mesh_draw(
                SceneObjectId(2),
                SceneGeometryId(5),
                "we/genericimage4",
                Some(SceneResourceId(7)),
            ),
            mesh_draw(
                SceneObjectId(3),
                SceneGeometryId(6),
                "we/genericimage4",
                Some(SceneResourceId(8)),
            ),
            additive_draw,
        ];
        let mut state = NativeVulkanSceneMeshDrawListState::default();

        let first = state
            .next_draw("scene-main", None, &draws[0])
            .expect("first draw");
        let second = state
            .next_draw("scene-main", None, &draws[1])
            .expect("second draw");
        let third = state
            .next_draw("scene-main", None, &draws[2])
            .expect("third draw");
        let fourth = state
            .next_draw("scene-main", None, &draws[3])
            .expect("fourth draw");

        assert!(first.bind_pipeline);
        assert!(!second.bind_pipeline);
        assert!(!third.bind_pipeline);
        assert!(fourth.bind_pipeline);
    }

    #[test]
    fn mesh_draw_list_accepts_puppet_skinning_draws_as_indexed_graphics() {
        let mut draw = mesh_draw(
            SceneObjectId(1),
            SceneGeometryId(4),
            "we/genericimage4",
            Some(SceneResourceId(7)),
        );
        draw.pipeline = SceneGraphPipelineClass::PuppetSkinning;
        draw.puppet = Some(crate::engine::scene_engine::ScenePuppetId(9));
        let mut state = NativeVulkanSceneMeshDrawListState::default();

        let transition = state
            .next_draw("scene-main", None, &draw)
            .expect("puppet draw must stay in indexed graphics batch");

        assert!(transition.bind_pipeline);
        assert_eq!(
            transition.pipeline_key.pipeline_class,
            SceneGraphPipelineClass::PuppetSkinning
        );
    }

    #[test]
    fn mesh_draw_list_rejects_non_indexed_graphics_draws_before_backend_recording() {
        let mut draw = mesh_draw(
            SceneObjectId(1),
            SceneGeometryId(4),
            "we/genericimage4",
            Some(SceneResourceId(7)),
        );
        draw.pipeline = SceneGraphPipelineClass::Quad;
        let mut state = NativeVulkanSceneMeshDrawListState::default();

        let err = state
            .next_draw("scene-main", None, &draw)
            .expect_err("non-indexed draw must fail");

        assert!(err.contains("requires indexed mesh graphics pipeline"));
    }

    fn mesh_draw(
        object: SceneObjectId,
        geometry: SceneGeometryId,
        shader: &str,
        resource: Option<SceneResourceId>,
    ) -> SceneGraphDraw {
        SceneGraphDraw {
            object,
            pipeline: SceneGraphPipelineClass::Mesh,
            material: SceneMaterialKey {
                shader: shader.to_owned(),
                blend: SceneBlendContract::TranslucentAlpha,
                render_state: crate::engine::scene_engine::SceneMaterialRenderState::translucent_2d(
                ),
            },
            geometry: Some(geometry),
            puppet: None,
            resources: resource
                .map(|resource| {
                    vec![SceneGraphResourceBinding {
                        slot: 0,
                        role: SceneGraphResourceRole::shader_texture(0),
                        resource,
                    }]
                })
                .unwrap_or_default(),
            index_count: 6,
        }
    }
}
