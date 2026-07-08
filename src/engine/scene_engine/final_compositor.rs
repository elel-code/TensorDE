//! Engine-owned ObjectFinal mesh compositor plan.
//!
//! References:
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/docs/exe/d3d11-context-calls.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/shaders/genericimage4.frag`
//! - `reverse-engineered/shaders/passthrough.frag`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/renderer_rd/renderer_canvas_render_rd.h`

use std::collections::BTreeMap;

use serde::Serialize;

use super::{
    SceneAlphaWriteMode, SceneBlendContract, SceneCullMode, SceneDepthTest,
    SceneEffectPassGraphOutput, SceneEffectPassGraphPlan, SceneGraphDraw, SceneGraphPass,
    SceneGraphPipelineClass, SceneGraphTarget, SceneMaterialKey, SceneMaterialRenderState,
    SceneObject, SceneObjectGeometry, SceneObjectId,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SceneFinalCompositorPlan {
    pub object_final_count: usize,
    pub pass_count: usize,
    pub object_finals: Vec<SceneObjectId>,
    pub object_inputs: Vec<SceneFinalCompositorObjectInput>,
    pub passes: Vec<SceneGraphPass>,
    pub command_order: [&'static str; 5],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SceneFinalCompositorObjectInput {
    pub object: SceneObjectId,
    pub input: SceneGraphTarget,
}

impl SceneFinalCompositorPlan {
    pub fn empty() -> Self {
        Self {
            object_final_count: 0,
            pass_count: 0,
            object_finals: Vec::new(),
            object_inputs: Vec::new(),
            passes: Vec::new(),
            command_order: final_compositor_command_order(),
        }
    }

    pub fn from_effect_pass_graph(
        objects: &[SceneObject],
        effect_graph: &SceneEffectPassGraphPlan,
    ) -> Self {
        let object_inputs = final_compositor_inputs(effect_graph);
        let mut plan = Self::empty();

        for object in objects
            .iter()
            .filter(|object| object_inputs.contains_key(&object.id))
        {
            let input = object_inputs[&object.id];
            plan.object_finals.push(object.id);
            plan.object_inputs.push(SceneFinalCompositorObjectInput {
                object: object.id,
                input,
            });
            plan.passes.push(final_compositor_pass(object, input));
        }

        plan.object_final_count = plan.object_finals.len();
        plan.pass_count = plan.passes.len();
        plan
    }

    pub fn contains_object(&self, object: SceneObjectId) -> bool {
        self.object_finals.contains(&object)
    }

    pub fn input_for_object(&self, object: SceneObjectId) -> Option<SceneGraphTarget> {
        self.object_inputs
            .iter()
            .find(|input| input.object == object)
            .map(|input| input.input)
    }
}

fn final_compositor_inputs(
    effect_graph: &SceneEffectPassGraphPlan,
) -> BTreeMap<SceneObjectId, SceneGraphTarget> {
    let mut inputs = BTreeMap::new();
    for target in &effect_graph.image_layer_targets {
        inputs.insert(target.object, target.final_source_target);
    }
    for pass in &effect_graph.passes {
        if let SceneEffectPassGraphOutput::ObjectFinal(object) = pass.output {
            inputs
                .entry(object)
                .or_insert(SceneGraphTarget::ObjectFinal(object));
        }
    }
    inputs
}

fn final_compositor_pass(object: &SceneObject, input: SceneGraphTarget) -> SceneGraphPass {
    SceneGraphPass {
        name: format!("scene-object-final-{}", object.id.0),
        input: Some(input),
        output: SceneGraphTarget::Swapchain,
        draws: vec![final_compositor_draw(object)],
    }
}

fn final_compositor_draw(object: &SceneObject) -> SceneGraphDraw {
    SceneGraphDraw {
        object: object.id,
        pipeline: scene_graph_pipeline_class(&object.geometry),
        material: object_final_material_key(object),
        geometry: scene_graph_geometry_id(&object.geometry),
        puppet: scene_graph_puppet_id(&object.geometry),
        resources: Vec::new(),
        index_count: object.geometry.index_count(),
    }
}

fn object_final_material_key(object: &SceneObject) -> SceneMaterialKey {
    SceneMaterialKey {
        shader: object.material.shader.clone(),
        blend: SceneBlendContract::NormalReplace,
        render_state: SceneMaterialRenderState {
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            alpha_write: SceneAlphaWriteMode::Default,
        },
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

fn final_compositor_command_order() -> [&'static str; 5] {
    [
        "collect_object_final_effect_outputs",
        "preserve_scene_object_order_for_final_composite",
        "emit_object_geometry_final_mesh_passes",
        "sample_final_graph_targets_as_g_texture0",
        "write_final_objects_to_swapchain_with_we_normal_replace",
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneEffectPassBlend, SceneEffectPassGraphMaterialPass, SceneGeometryId,
        SceneMaterialContract, SceneResourceId, we::WeEffectKind,
    };

    #[test]
    fn final_compositor_collects_object_final_outputs_in_object_order() {
        let objects = vec![
            object(SceneObjectId(3)),
            object(SceneObjectId(1)),
            object(SceneObjectId(2)),
        ];
        let effect_graph = SceneEffectPassGraphPlan {
            object_program_count: 2,
            material_pass_count: 2,
            copy_command_count: 1,
            swap_command_count: 1,
            passes: vec![
                effect_output_pass(0, SceneObjectId(1)),
                effect_output_pass(1, SceneObjectId(3)),
            ],
            ..SceneEffectPassGraphPlan::empty()
        };

        let plan = SceneFinalCompositorPlan::from_effect_pass_graph(&objects, &effect_graph);

        assert_eq!(plan.object_final_count, 2);
        assert_eq!(plan.pass_count, 2);
        assert_eq!(plan.object_finals, vec![SceneObjectId(3), SceneObjectId(1)]);
        assert_eq!(plan.passes[0].name, "scene-object-final-3");
        assert_eq!(
            plan.passes[0].input,
            Some(SceneGraphTarget::ObjectFinal(SceneObjectId(3)))
        );
        assert_eq!(plan.passes[0].output, SceneGraphTarget::Swapchain);
        assert_eq!(plan.passes[0].draws[0].object, SceneObjectId(3));
        assert_eq!(plan.passes[0].draws[0].resources, Vec::new());
        assert_eq!(
            plan.passes[0].draws[0].material.blend,
            SceneBlendContract::NormalReplace
        );
        assert_eq!(
            plan.passes[0].draws[0].material.render_state.alpha_write,
            SceneAlphaWriteMode::Default
        );
    }

    #[test]
    fn final_compositor_marks_object_membership_without_sorting_by_id() {
        let objects = vec![object(SceneObjectId(7))];
        let effect_graph = SceneEffectPassGraphPlan {
            object_program_count: 1,
            material_pass_count: 1,
            passes: vec![effect_output_pass(0, SceneObjectId(7))],
            ..SceneEffectPassGraphPlan::empty()
        };

        let plan = SceneFinalCompositorPlan::from_effect_pass_graph(&objects, &effect_graph);

        assert!(plan.contains_object(SceneObjectId(7)));
        assert!(!plan.contains_object(SceneObjectId(8)));
    }

    #[test]
    fn final_compositor_samples_image_layer_composite_a_for_scene_output_effects() {
        let object_id = SceneObjectId(1530);
        let objects = vec![object(object_id)];
        let image_layer_target =
            crate::engine::scene_engine::SceneImageLayerTargetPlan::for_object(
                object_id,
                Some(SceneResourceId(77)),
                1,
            )
            .expect("image layer target plan");
        let effect_graph = SceneEffectPassGraphPlan {
            object_program_count: 1,
            material_pass_count: 1,
            image_layer_target_count: 1,
            image_layer_scene_output_pass_count: 1,
            image_layer_targets: vec![image_layer_target],
            passes: vec![SceneEffectPassGraphMaterialPass {
                output: SceneEffectPassGraphOutput::GraphTarget(
                    SceneGraphTarget::ImageLayerCompositeA(object_id),
                ),
                ..effect_output_pass(0, object_id)
            }],
            ..SceneEffectPassGraphPlan::empty()
        };

        let plan = SceneFinalCompositorPlan::from_effect_pass_graph(&objects, &effect_graph);

        assert_eq!(plan.object_finals, vec![object_id]);
        assert_eq!(
            plan.input_for_object(object_id),
            Some(SceneGraphTarget::ImageLayerCompositeA(object_id))
        );
        assert_eq!(
            plan.passes[0].input,
            Some(SceneGraphTarget::ImageLayerCompositeA(object_id))
        );
    }

    fn effect_output_pass(
        graph_pass_index: usize,
        object: SceneObjectId,
    ) -> SceneEffectPassGraphMaterialPass {
        SceneEffectPassGraphMaterialPass {
            graph_command_index: graph_pass_index,
            graph_pass_index,
            object,
            program_index: graph_pass_index,
            pass_index: graph_pass_index,
            effect_file: "effects/iris/effect.json".to_owned(),
            effect: WeEffectKind::Iris,
            shader: Some("effects/iris".to_owned()),
            source: None,
            input_bindings: Vec::new(),
            output: SceneEffectPassGraphOutput::ObjectFinal(object),
            blend: SceneEffectPassBlend::NormalReplace,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            alpha_write: SceneAlphaWriteMode::Default,
            texture_resources: Vec::new(),
            combos: Default::default(),
            constants: Default::default(),
        }
    }

    fn object(id: SceneObjectId) -> SceneObject {
        SceneObject {
            id,
            geometry: SceneObjectGeometry::Mesh {
                geometry: SceneGeometryId(id.0),
                vertex_count: 4,
                index_count: 6,
            },
            material: SceneMaterialContract {
                shader: "we/genericimage4".to_owned(),
                blend: SceneBlendContract::TranslucentAlpha,
                render_state: SceneMaterialRenderState::translucent_2d(),
            },
            source: Some(SceneResourceId(id.0)),
        }
    }
}
