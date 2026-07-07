//! Native Vulkan scene renderer graph builder.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`

use crate::engine::scene_engine::{
    RendererSceneRender, SceneFrameContext, SceneGraph, SceneGraphDraw, SceneGraphPass,
    SceneGraphPipelineClass, SceneGraphResourceBinding, SceneGraphResourceRole, SceneGraphTarget,
    SceneObject, SceneObjectEffectProgram, SceneObjectGeometry, SceneResource,
};

#[derive(Debug, Default)]
pub struct NativeVulkanRendererSceneRender;

impl NativeVulkanRendererSceneRender {
    pub fn new() -> Self {
        Self
    }
}

impl RendererSceneRender for NativeVulkanRendererSceneRender {
    fn build_graph(
        &self,
        _context: SceneFrameContext,
        _resources: &[SceneResource],
        objects: &[SceneObject],
        _effects: &[SceneObjectEffectProgram],
    ) -> Result<SceneGraph, String> {
        if objects.is_empty() {
            return Ok(SceneGraph::default());
        }

        Ok(SceneGraph {
            passes: vec![SceneGraphPass {
                name: "scene-main".to_owned(),
                input: None,
                output: SceneGraphTarget::Swapchain,
                draws: objects.iter().map(scene_graph_draw_for_object).collect(),
            }],
        })
    }
}

fn scene_graph_draw_for_object(object: &SceneObject) -> SceneGraphDraw {
    SceneGraphDraw {
        object: object.id,
        pipeline: scene_graph_pipeline_class(&object.geometry),
        material: object.material.key(),
        geometry: scene_graph_geometry_id(&object.geometry),
        puppet: scene_graph_puppet_id(&object.geometry),
        resources: object
            .source
            .map(|resource| {
                vec![SceneGraphResourceBinding {
                    slot: 0,
                    role: SceneGraphResourceRole::shader_texture(0),
                    resource,
                }]
            })
            .unwrap_or_default(),
        index_count: object.geometry.index_count(),
    }
}

fn scene_graph_pipeline_class(geometry: &SceneObjectGeometry) -> SceneGraphPipelineClass {
    match geometry {
        SceneObjectGeometry::Quad => SceneGraphPipelineClass::Quad,
        SceneObjectGeometry::Mesh { .. } => SceneGraphPipelineClass::Mesh,
        SceneObjectGeometry::Puppet { .. } => SceneGraphPipelineClass::PuppetSkinning,
        SceneObjectGeometry::ParticleEmitter => SceneGraphPipelineClass::ParticleEmitter,
    }
}

fn scene_graph_geometry_id(
    geometry: &SceneObjectGeometry,
) -> Option<crate::engine::scene_engine::SceneGeometryId> {
    match geometry {
        SceneObjectGeometry::Mesh { geometry, .. }
        | SceneObjectGeometry::Puppet { geometry, .. } => Some(*geometry),
        SceneObjectGeometry::Quad | SceneObjectGeometry::ParticleEmitter => None,
    }
}

fn scene_graph_puppet_id(
    geometry: &SceneObjectGeometry,
) -> Option<crate::engine::scene_engine::ScenePuppetId> {
    match geometry {
        SceneObjectGeometry::Puppet { puppet, .. } => Some(*puppet),
        SceneObjectGeometry::Quad
        | SceneObjectGeometry::Mesh { .. }
        | SceneObjectGeometry::ParticleEmitter => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneFrameContext, SceneGeometryId, SceneMaterialContract,
        SceneObjectId, SceneResourceId,
    };

    #[test]
    fn graph_draw_keeps_geometry_index_count_and_resource_binding() {
        let renderer = NativeVulkanRendererSceneRender::new();
        let object = SceneObject {
            id: SceneObjectId(9),
            geometry: SceneObjectGeometry::Mesh {
                geometry: SceneGeometryId(3),
                vertex_count: 24,
                index_count: 42,
            },
            material: SceneMaterialContract {
                shader: "we/genericimage4".to_owned(),
                blend: SceneBlendContract::TranslucentAlpha,
                render_state: crate::engine::scene_engine::SceneMaterialRenderState::translucent_2d(
                ),
            },
            source: Some(SceneResourceId(7)),
        };
        let graph = renderer
            .build_graph(
                SceneFrameContext {
                    time_ms: 250,
                    target_width: 3840,
                    target_height: 2160,
                },
                &[],
                &[object],
                &[],
            )
            .expect("scene graph");

        assert_eq!(graph.passes.len(), 1);
        assert_eq!(graph.passes[0].name, "scene-main");
        assert_eq!(graph.passes[0].input, None);
        let draw = &graph.passes[0].draws[0];
        assert_eq!(draw.pipeline, SceneGraphPipelineClass::Mesh);
        assert_eq!(draw.geometry, Some(SceneGeometryId(3)));
        assert_eq!(draw.index_count, 42);
        assert_eq!(draw.resources[0].resource, SceneResourceId(7));
    }

    #[test]
    fn graph_batches_objects_into_one_scene_main_pass_without_reordering_draws() {
        let renderer = NativeVulkanRendererSceneRender::new();
        let objects = vec![
            mesh_object(SceneObjectId(1), SceneGeometryId(3), SceneResourceId(7)),
            mesh_object(SceneObjectId(2), SceneGeometryId(4), SceneResourceId(8)),
        ];

        let graph = renderer
            .build_graph(
                SceneFrameContext {
                    time_ms: 250,
                    target_width: 3840,
                    target_height: 2160,
                },
                &[],
                &objects,
                &[],
            )
            .expect("scene graph");

        assert_eq!(graph.passes.len(), 1);
        assert_eq!(graph.passes[0].name, "scene-main");
        assert_eq!(graph.passes[0].draws.len(), 2);
        assert_eq!(graph.passes[0].draws[0].object, SceneObjectId(1));
        assert_eq!(graph.passes[0].draws[0].geometry, Some(SceneGeometryId(3)));
        assert_eq!(
            graph.passes[0].draws[0].resources[0].resource,
            SceneResourceId(7)
        );
        assert_eq!(graph.passes[0].draws[1].object, SceneObjectId(2));
        assert_eq!(graph.passes[0].draws[1].geometry, Some(SceneGeometryId(4)));
        assert_eq!(
            graph.passes[0].draws[1].resources[0].resource,
            SceneResourceId(8)
        );
    }

    #[test]
    fn graph_has_no_empty_scene_pass() {
        let renderer = NativeVulkanRendererSceneRender::new();

        let graph = renderer
            .build_graph(
                SceneFrameContext {
                    time_ms: 250,
                    target_width: 3840,
                    target_height: 2160,
                },
                &[],
                &[],
                &[],
            )
            .expect("scene graph");

        assert!(graph.passes.is_empty());
    }

    fn mesh_object(
        id: SceneObjectId,
        geometry: SceneGeometryId,
        source: SceneResourceId,
    ) -> SceneObject {
        SceneObject {
            id,
            geometry: SceneObjectGeometry::Mesh {
                geometry,
                vertex_count: 24,
                index_count: 42,
            },
            material: SceneMaterialContract {
                shader: "we/genericimage4".to_owned(),
                blend: SceneBlendContract::TranslucentAlpha,
                render_state: crate::engine::scene_engine::SceneMaterialRenderState::translucent_2d(
                ),
            },
            source: Some(source),
        }
    }
}
