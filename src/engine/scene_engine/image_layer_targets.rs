//! WE image-layer source/composite target routing.
//!
//! References:
//! - `reverse-engineered/docs/exe/TODO.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `reverse-engineered/tools/audit_opacity_final_alpha_path.py`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/renderer_rd/renderer_canvas_render_rd.h`

use serde::Serialize;

use super::{SceneGraphTarget, SceneObjectId, SceneResourceId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneImageLayerTargetPlan {
    pub object: SceneObjectId,
    pub source: Option<SceneResourceId>,
    pub scene_output_pass_count: usize,
    pub prefill_target: SceneGraphTarget,
    pub final_source_target: SceneGraphTarget,
    pub pass_targets: Vec<SceneImageLayerPassTarget>,
    pub command_order: [&'static str; 6],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SceneImageLayerPassTarget {
    pub scene_output_pass_index: usize,
    pub source: SceneGraphTarget,
    pub output: SceneGraphTarget,
}

impl SceneImageLayerTargetPlan {
    pub fn for_object(
        object: SceneObjectId,
        source: Option<SceneResourceId>,
        scene_output_pass_count: usize,
    ) -> Option<Self> {
        if scene_output_pass_count == 0 {
            return None;
        }
        let pass_targets = (0..scene_output_pass_count)
            .map(|scene_output_pass_index| {
                image_layer_pass_target(object, scene_output_pass_count, scene_output_pass_index)
            })
            .collect::<Vec<_>>();
        Some(Self {
            object,
            source,
            scene_output_pass_count,
            prefill_target: image_layer_prefill_target(object, scene_output_pass_count),
            final_source_target: SceneGraphTarget::ImageLayerCompositeA(object),
            pass_targets,
            command_order: [
                "read_object_effect_scene_output_pass_count",
                "apply_0x1401e9513_pass_count_parity_initial_target",
                "alternate_image_layer_source_and_composite_targets",
                "preserve_0x1401e964f_source_target_prefill",
                "resolve_0x1401e9ff3_final_composite_source",
                "emit_godot_style_per_object_render_targets",
            ],
        })
    }

    pub fn pass_target(&self, scene_output_pass_index: usize) -> Option<SceneImageLayerPassTarget> {
        self.pass_targets
            .iter()
            .copied()
            .find(|target| target.scene_output_pass_index == scene_output_pass_index)
    }
}

pub fn image_layer_pass_target(
    object: SceneObjectId,
    scene_output_pass_count: usize,
    scene_output_pass_index: usize,
) -> SceneImageLayerPassTarget {
    let output = if (scene_output_pass_count.saturating_sub(scene_output_pass_index)) & 1 == 1 {
        SceneGraphTarget::ImageLayerCompositeA(object)
    } else {
        SceneGraphTarget::ImageLayerSource(object)
    };
    let source = match output {
        SceneGraphTarget::ImageLayerCompositeA(_) => SceneGraphTarget::ImageLayerSource(object),
        SceneGraphTarget::ImageLayerSource(_) => SceneGraphTarget::ImageLayerCompositeA(object),
        _ => unreachable!("image-layer output target must be one of the object-local pair"),
    };
    SceneImageLayerPassTarget {
        scene_output_pass_index,
        source,
        output,
    }
}

pub fn image_layer_prefill_target(
    object: SceneObjectId,
    scene_output_pass_count: usize,
) -> SceneGraphTarget {
    if scene_output_pass_count & 1 == 1 {
        SceneGraphTarget::ImageLayerSource(object)
    } else {
        SceneGraphTarget::ImageLayerCompositeA(object)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_layer_target_plan_matches_we_one_pass_route() {
        let object = SceneObjectId(1530);
        let plan = SceneImageLayerTargetPlan::for_object(object, Some(SceneResourceId(9)), 1)
            .expect("one pass image-layer target plan");

        assert_eq!(
            plan.prefill_target,
            SceneGraphTarget::ImageLayerSource(object)
        );
        assert_eq!(
            plan.final_source_target,
            SceneGraphTarget::ImageLayerCompositeA(object)
        );
        assert_eq!(
            plan.pass_targets,
            vec![SceneImageLayerPassTarget {
                scene_output_pass_index: 0,
                source: SceneGraphTarget::ImageLayerSource(object),
                output: SceneGraphTarget::ImageLayerCompositeA(object),
            }]
        );
    }

    #[test]
    fn image_layer_target_plan_matches_we_two_pass_route() {
        let object = SceneObjectId(1336);
        let plan = SceneImageLayerTargetPlan::for_object(object, Some(SceneResourceId(7)), 2)
            .expect("two pass image-layer target plan");

        assert_eq!(
            plan.prefill_target,
            SceneGraphTarget::ImageLayerCompositeA(object)
        );
        assert_eq!(
            plan.final_source_target,
            SceneGraphTarget::ImageLayerCompositeA(object)
        );
        assert_eq!(
            plan.pass_targets,
            vec![
                SceneImageLayerPassTarget {
                    scene_output_pass_index: 0,
                    source: SceneGraphTarget::ImageLayerCompositeA(object),
                    output: SceneGraphTarget::ImageLayerSource(object),
                },
                SceneImageLayerPassTarget {
                    scene_output_pass_index: 1,
                    source: SceneGraphTarget::ImageLayerSource(object),
                    output: SceneGraphTarget::ImageLayerCompositeA(object),
                },
            ]
        );
    }
}
