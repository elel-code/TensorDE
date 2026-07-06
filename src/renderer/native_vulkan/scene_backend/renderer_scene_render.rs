//! Native Vulkan scene renderer graph builder.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`

use crate::engine::scene_engine::{
    RendererSceneRender, SceneFrameContext, SceneGraph, SceneGraphDraw, SceneGraphPass,
    SceneGraphPipelineClass, SceneGraphResourceBinding, SceneGraphResourceRole, SceneGraphTarget,
    SceneObject, SceneObjectGeometry, SceneResource,
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
    ) -> SceneGraph {
        let passes = objects.iter().map(scene_graph_pass_for_object).collect();
        SceneGraph { passes }
    }
}

fn scene_graph_pass_for_object(object: &SceneObject) -> SceneGraphPass {
    SceneGraphPass {
        name: format!("object-{}", object.id.0),
        input: object
            .source
            .map(|source| SceneGraphTarget::ImageLocalMain(source.0)),
        output: SceneGraphTarget::Swapchain,
        draws: vec![SceneGraphDraw {
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
                        role: SceneGraphResourceRole::BaseColor,
                        resource,
                    }]
                })
                .unwrap_or_default(),
            index_count: object.geometry.index_count(),
        }],
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
                writes_depth: false,
                tests_depth: false,
            },
            source: Some(SceneResourceId(7)),
        };
        let graph = renderer.build_graph(
            SceneFrameContext {
                time_ms: 250,
                target_width: 3840,
                target_height: 2160,
            },
            &[],
            &[object],
        );

        assert_eq!(graph.passes.len(), 1);
        let draw = &graph.passes[0].draws[0];
        assert_eq!(draw.pipeline, SceneGraphPipelineClass::Mesh);
        assert_eq!(draw.geometry, Some(SceneGeometryId(3)));
        assert_eq!(draw.index_count, 42);
        assert_eq!(draw.resources[0].resource, SceneResourceId(7));
    }
}
