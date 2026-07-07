//! Native Vulkan scene renderer graph builder.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`

use std::collections::BTreeMap;

use crate::engine::scene_engine::{
    RendererSceneRender, SceneEffectPassGraphPlan, SceneFinalCompositorPlan, SceneFrameContext,
    SceneGraph, SceneGraphDraw, SceneGraphPass, SceneGraphPipelineClass, SceneGraphResourceBinding,
    SceneGraphResourceRole, SceneGraphTarget, SceneLayerCompositorPlan, SceneObject,
    SceneObjectEffectProgram, SceneObjectGeometry, SceneResource,
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
        _effect_pass_graph: &SceneEffectPassGraphPlan,
        final_compositor: &SceneFinalCompositorPlan,
        layer_compositor: &SceneLayerCompositorPlan,
    ) -> Result<SceneGraph, String> {
        if objects.is_empty() {
            return Ok(SceneGraph::default());
        }

        if layer_compositor.object_final_layer_count == 0 {
            return Ok(SceneGraph {
                passes: vec![direct_scene_pass(
                    0,
                    objects.iter().map(scene_graph_draw_for_object),
                )],
            });
        }

        let final_passes_by_object = final_compositor
            .passes
            .iter()
            .filter_map(|pass| pass.draws.first().map(|draw| (draw.object, pass)))
            .collect::<BTreeMap<_, _>>();
        let mut passes = Vec::new();
        let mut direct_draws = Vec::new();
        for object in objects {
            let routes_object_final = layer_compositor
                .layer_for_object(object.id)
                .is_some_and(|layer| layer.routes_object_final());
            if routes_object_final {
                let final_pass = final_passes_by_object.get(&object.id).ok_or_else(|| {
                    format!(
                        "scene layer compositor routes object {:?} through ObjectFinal but final compositor has no mesh pass",
                        object.id
                    )
                })?;
                flush_direct_scene_pass(&mut passes, &mut direct_draws);
                passes.push((*final_pass).clone());
            } else {
                direct_draws.push(scene_graph_draw_for_object(object));
            }
        }
        flush_direct_scene_pass(&mut passes, &mut direct_draws);

        Ok(SceneGraph { passes })
    }
}

fn flush_direct_scene_pass(
    passes: &mut Vec<SceneGraphPass>,
    direct_draws: &mut Vec<SceneGraphDraw>,
) {
    if direct_draws.is_empty() {
        return;
    }
    let pass_index = passes.len();
    let draws = std::mem::take(direct_draws);
    passes.push(direct_scene_pass(pass_index, draws));
}

fn direct_scene_pass(
    pass_index: usize,
    draws: impl IntoIterator<Item = SceneGraphDraw>,
) -> SceneGraphPass {
    SceneGraphPass {
        name: if pass_index == 0 {
            "scene-main".to_owned()
        } else {
            format!("scene-main-{pass_index}")
        },
        input: None,
        output: SceneGraphTarget::Swapchain,
        draws: draws.into_iter().collect(),
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
        SceneBlendContract, SceneEffectPassBlend, SceneEffectPassGraphMaterialPass,
        SceneEffectPassGraphOutput, SceneFrameContext, SceneGeometryId, SceneMaterialContract,
        SceneObjectId, SceneResourceId, we::WeEffectKind,
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
                &SceneEffectPassGraphPlan::empty(),
                &SceneFinalCompositorPlan::empty(),
                &SceneLayerCompositorPlan::empty(),
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
                &SceneEffectPassGraphPlan::empty(),
                &SceneFinalCompositorPlan::empty(),
                &SceneLayerCompositorPlan::empty(),
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
                &SceneEffectPassGraphPlan::empty(),
                &SceneFinalCompositorPlan::empty(),
                &SceneLayerCompositorPlan::empty(),
            )
            .expect("scene graph");

        assert!(graph.passes.is_empty());
    }

    #[test]
    fn graph_routes_effect_objects_through_object_final_mesh_passes() {
        let renderer = NativeVulkanRendererSceneRender::new();
        let objects = vec![
            mesh_object(SceneObjectId(1), SceneGeometryId(3), SceneResourceId(7)),
            mesh_object(SceneObjectId(2), SceneGeometryId(4), SceneResourceId(8)),
            mesh_object(SceneObjectId(3), SceneGeometryId(5), SceneResourceId(9)),
        ];
        let effect_graph = SceneEffectPassGraphPlan {
            material_pass_count: 1,
            passes: vec![effect_output_pass(SceneObjectId(2))],
            ..SceneEffectPassGraphPlan::empty()
        };
        let final_compositor =
            SceneFinalCompositorPlan::from_effect_pass_graph(&objects, &effect_graph);
        let layer_compositor =
            SceneLayerCompositorPlan::from_scene(&[], &objects, &effect_graph, &final_compositor);

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
                &effect_graph,
                &final_compositor,
                &layer_compositor,
            )
            .expect("scene graph");

        assert_eq!(graph.passes.len(), 3);
        assert_eq!(graph.passes[0].draws[0].object, SceneObjectId(1));
        assert_eq!(graph.passes[1].name, "scene-object-final-2");
        assert_eq!(
            graph.passes[1].input,
            Some(SceneGraphTarget::ObjectFinal(SceneObjectId(2)))
        );
        assert_eq!(graph.passes[1].draws[0].object, SceneObjectId(2));
        assert_eq!(graph.passes[1].draws[0].resources, Vec::new());
        assert_eq!(
            graph.passes[1].draws[0].material.blend,
            SceneBlendContract::NormalReplace
        );
        assert_eq!(graph.passes[2].draws[0].object, SceneObjectId(3));
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

    fn effect_output_pass(object: SceneObjectId) -> SceneEffectPassGraphMaterialPass {
        SceneEffectPassGraphMaterialPass {
            graph_command_index: 0,
            graph_pass_index: 0,
            object,
            program_index: 0,
            pass_index: 0,
            effect_file: "effects/iris/effect.json".to_owned(),
            effect: WeEffectKind::Iris,
            shader: Some("effects/iris".to_owned()),
            source: None,
            input_bindings: Vec::new(),
            output: SceneEffectPassGraphOutput::ObjectFinal(object),
            blend: SceneEffectPassBlend::NormalReplace,
            depth_test: crate::engine::scene_engine::SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: crate::engine::scene_engine::SceneCullMode::None,
            alpha_write: crate::engine::scene_engine::SceneAlphaWriteMode::Default,
            texture_resources: Vec::new(),
            combos: Default::default(),
            constants: Default::default(),
        }
    }
}
