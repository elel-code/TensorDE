//! WE image-effect graph lowering into scene-engine targets.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/index.md`
//! - `reverse-engineered/effects/iris.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/renderer_scene_render.h`

use serde::Serialize;

use super::{
    SceneBlendContract, SceneGraphTarget,
    we::{
        WeEffectKind, WeEffectOutputContract, WeImageGraph, WePassBlendMove, WePassRole, WeTarget,
    },
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneEffectGraphPlan {
    pub pass_count: usize,
    pub render_target_count: usize,
    pub source_read_count: usize,
    pub graph_target_read_count: usize,
    pub passes: Vec<SceneEffectPassPlan>,
    pub render_targets: Vec<SceneEffectTargetBinding>,
    pub command_order: [&'static str; 4],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneEffectPassPlan {
    pub pass_index: usize,
    pub role: WePassRole,
    pub shader_family: SceneEffectShaderFamily,
    pub input: SceneEffectInput,
    pub output: SceneGraphTarget,
    pub blend: SceneBlendContract,
    pub output_contract: WeEffectOutputContract,
    pub blend_move: WePassBlendMove,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SceneEffectShaderFamily {
    BaseMaterial,
    BuiltInEffect(WeEffectKind),
    ColorBlendPassthrough,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SceneEffectInput {
    SourceTexture,
    GraphTarget(SceneGraphTarget),
    Scene,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SceneEffectTargetBinding {
    pub we_target: WeTarget,
    pub scene_target: SceneGraphTarget,
}

impl SceneEffectGraphPlan {
    pub fn from_we_image_graph(graph: &WeImageGraph) -> Result<Self, String> {
        let mut passes = Vec::with_capacity(graph.steps.len());
        let mut render_targets = Vec::new();
        let mut source_read_count = 0usize;
        let mut graph_target_read_count = 0usize;

        for (pass_index, step) in graph.steps.iter().enumerate() {
            let input = scene_effect_input(step.input)?;
            match input {
                SceneEffectInput::SourceTexture => {
                    source_read_count = source_read_count.saturating_add(1);
                }
                SceneEffectInput::GraphTarget(_) => {
                    graph_target_read_count = graph_target_read_count.saturating_add(1);
                }
                SceneEffectInput::Scene => {}
            }

            let output = scene_effect_render_target(step.output)?;
            push_unique_target(&mut render_targets, step.output, output);

            let shader_family = scene_effect_shader_family(step.role, step.effect)?;
            let output_contract = scene_effect_output_contract(shader_family);
            passes.push(SceneEffectPassPlan {
                pass_index,
                role: step.role,
                shader_family,
                input,
                output,
                blend: step.blend,
                output_contract,
                blend_move: scene_effect_blend_move(output_contract),
            });
        }

        Ok(Self {
            pass_count: passes.len(),
            render_target_count: render_targets.len(),
            source_read_count,
            graph_target_read_count,
            passes,
            render_targets,
            command_order: [
                "lower_we_effect_steps",
                "resolve_effect_sources",
                "resolve_effect_render_targets",
                "preserve_we_effect_output_contracts",
            ],
        })
    }
}

fn scene_effect_shader_family(
    role: WePassRole,
    effect: Option<WeEffectKind>,
) -> Result<SceneEffectShaderFamily, String> {
    match (role, effect) {
        (WePassRole::BaseMaterial, None) => Ok(SceneEffectShaderFamily::BaseMaterial),
        (WePassRole::EffectMaterial, Some(effect)) => {
            Ok(SceneEffectShaderFamily::BuiltInEffect(effect))
        }
        (WePassRole::ColorBlendPassthrough, None) => {
            Ok(SceneEffectShaderFamily::ColorBlendPassthrough)
        }
        (WePassRole::EffectMaterial, None) => {
            Err("WE effect material pass requires an effect kind from effect.json".to_owned())
        }
        (role, Some(effect)) => Err(format!(
            "WE image graph role {role:?} cannot carry built-in effect {effect:?}"
        )),
    }
}

fn scene_effect_input(target: WeTarget) -> Result<SceneEffectInput, String> {
    match target {
        WeTarget::SourceTexture => Ok(SceneEffectInput::SourceTexture),
        WeTarget::ImageLocalMain => Ok(SceneEffectInput::GraphTarget(
            SceneGraphTarget::ImageLocalMain(0),
        )),
        WeTarget::ImageLocalSub => Ok(SceneEffectInput::GraphTarget(
            SceneGraphTarget::ImageLocalSub(0),
        )),
        WeTarget::NamedFbo(index) => Ok(SceneEffectInput::GraphTarget(SceneGraphTarget::NamedFbo(
            index,
        ))),
        WeTarget::FirstClassEffectTarget => Ok(SceneEffectInput::GraphTarget(
            SceneGraphTarget::EffectTarget(0),
        )),
        WeTarget::Scene => Ok(SceneEffectInput::Scene),
    }
}

fn scene_effect_render_target(target: WeTarget) -> Result<SceneGraphTarget, String> {
    match target {
        WeTarget::SourceTexture => {
            Err("WE source texture is an effect input, not a render target".to_owned())
        }
        WeTarget::ImageLocalMain => Ok(SceneGraphTarget::ImageLocalMain(0)),
        WeTarget::ImageLocalSub => Ok(SceneGraphTarget::ImageLocalSub(0)),
        WeTarget::NamedFbo(index) => Ok(SceneGraphTarget::NamedFbo(index)),
        WeTarget::FirstClassEffectTarget => Ok(SceneGraphTarget::EffectTarget(0)),
        WeTarget::Scene => Ok(SceneGraphTarget::Swapchain),
    }
}

fn scene_effect_output_contract(shader_family: SceneEffectShaderFamily) -> WeEffectOutputContract {
    match shader_family {
        SceneEffectShaderFamily::BaseMaterial => WeEffectOutputContract::Replacement,
        SceneEffectShaderFamily::BuiltInEffect(effect) => effect.output_contract(),
        SceneEffectShaderFamily::ColorBlendPassthrough => WeEffectOutputContract::ColorBlend,
    }
}

fn scene_effect_blend_move(output: WeEffectOutputContract) -> WePassBlendMove {
    match output {
        WeEffectOutputContract::SourcePreserving => WePassBlendMove::MoveToFinalScenePass,
        WeEffectOutputContract::AlphaModifying
        | WeEffectOutputContract::ColorBlend
        | WeEffectOutputContract::Replacement => WePassBlendMove::KeepOnFirstPass,
    }
}

fn push_unique_target(
    targets: &mut Vec<SceneEffectTargetBinding>,
    we_target: WeTarget,
    scene_target: SceneGraphTarget,
) {
    if targets
        .iter()
        .any(|target| target.scene_target == scene_target && target.we_target == we_target)
    {
        return;
    }
    targets.push(SceneEffectTargetBinding {
        we_target,
        scene_target,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::we::{WeImageGraphStep, WePassRole};

    #[test]
    fn effect_graph_lowers_iris_as_source_preserving_target_pass() {
        let graph = WeImageGraph {
            steps: vec![
                WeImageGraphStep {
                    role: WePassRole::EffectMaterial,
                    effect: Some(WeEffectKind::Iris),
                    input: WeTarget::SourceTexture,
                    output: WeTarget::FirstClassEffectTarget,
                    blend: SceneBlendContract::NormalReplace,
                },
                WeImageGraphStep {
                    role: WePassRole::ColorBlendPassthrough,
                    effect: None,
                    input: WeTarget::FirstClassEffectTarget,
                    output: WeTarget::Scene,
                    blend: SceneBlendContract::TranslucentAlpha,
                },
            ],
        };

        let plan = SceneEffectGraphPlan::from_we_image_graph(&graph).expect("effect graph");

        assert_eq!(plan.pass_count, 2);
        assert_eq!(plan.source_read_count, 1);
        assert_eq!(plan.graph_target_read_count, 1);
        assert_eq!(plan.render_target_count, 2);
        assert_eq!(plan.passes[0].input, SceneEffectInput::SourceTexture);
        assert_eq!(plan.passes[0].output, SceneGraphTarget::EffectTarget(0));
        assert_eq!(
            plan.passes[0].output_contract,
            WeEffectOutputContract::SourcePreserving
        );
        assert_eq!(
            plan.passes[0].blend_move,
            WePassBlendMove::MoveToFinalScenePass
        );
        assert_eq!(
            plan.passes[1].input,
            SceneEffectInput::GraphTarget(SceneGraphTarget::EffectTarget(0))
        );
        assert_eq!(plan.passes[1].output, SceneGraphTarget::Swapchain);
        assert_eq!(
            plan.render_targets,
            vec![
                SceneEffectTargetBinding {
                    we_target: WeTarget::FirstClassEffectTarget,
                    scene_target: SceneGraphTarget::EffectTarget(0),
                },
                SceneEffectTargetBinding {
                    we_target: WeTarget::Scene,
                    scene_target: SceneGraphTarget::Swapchain,
                },
            ]
        );
    }

    #[test]
    fn effect_graph_rejects_source_texture_as_output() {
        let graph = WeImageGraph {
            steps: vec![WeImageGraphStep {
                role: WePassRole::EffectMaterial,
                effect: Some(WeEffectKind::WaterFlow),
                input: WeTarget::SourceTexture,
                output: WeTarget::SourceTexture,
                blend: SceneBlendContract::NormalReplace,
            }],
        };

        let err = SceneEffectGraphPlan::from_we_image_graph(&graph)
            .expect_err("source texture cannot be an output target");

        assert!(err.contains("not a render target"));
    }

    #[test]
    fn effect_graph_rejects_effect_pass_without_effect_kind() {
        let graph = WeImageGraph {
            steps: vec![WeImageGraphStep {
                role: WePassRole::EffectMaterial,
                effect: None,
                input: WeTarget::SourceTexture,
                output: WeTarget::FirstClassEffectTarget,
                blend: SceneBlendContract::NormalReplace,
            }],
        };

        let err = SceneEffectGraphPlan::from_we_image_graph(&graph)
            .expect_err("effect pass must name an effect");

        assert!(err.contains("requires an effect kind"));
    }
}
