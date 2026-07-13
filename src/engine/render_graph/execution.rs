use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use super::graph::RenderGraph;
use super::pass::{RenderPassNode, RenderPassRole};
use super::target::RenderTargetRole;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderGraphExecutionPlan {
    pub pass_count: u32,
    pub dependency_count: u32,
    pub level_count: u32,
    pub levels: Vec<RenderGraphExecutionLevel>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderGraphExecutionLevel {
    pub level: u32,
    pub passes: Vec<RenderGraphExecutionPass>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderGraphExecutionPass {
    pub pass_id: u32,
    pub role: RenderPassRole,
    pub target: RenderTargetRole,
    pub target_name: Option<String>,
    pub priority: u32,
}

impl RenderGraph {
    pub fn execution_plan(&self) -> RenderGraphExecutionPlan {
        if self.passes.is_empty() {
            return RenderGraphExecutionPlan::default();
        }

        let pass_indices_by_id = self
            .passes
            .iter()
            .enumerate()
            .map(|(index, pass)| (pass.id, index))
            .collect::<BTreeMap<_, _>>();
        let dependencies = self.execution_dependencies(&pass_indices_by_id);
        let levels_by_pass_index =
            self.execution_levels_by_pass_index(&dependencies, &pass_indices_by_id);
        let mut passes_by_level = BTreeMap::<u32, Vec<RenderGraphExecutionPass>>::new();
        for (index, pass) in self.passes.iter().enumerate() {
            let level = levels_by_pass_index
                .get(index)
                .copied()
                .unwrap_or(index.min(u32::MAX as usize) as u32);
            passes_by_level
                .entry(level)
                .or_default()
                .push(render_graph_execution_pass(pass));
        }
        let levels = passes_by_level
            .into_iter()
            .map(|(level, mut passes)| {
                passes.sort_by(|left, right| {
                    left.priority
                        .cmp(&right.priority)
                        .then(left.pass_id.cmp(&right.pass_id))
                });
                RenderGraphExecutionLevel { level, passes }
            })
            .collect::<Vec<_>>();

        RenderGraphExecutionPlan {
            pass_count: self.passes.len().min(u32::MAX as usize) as u32,
            dependency_count: dependencies.len().min(u32::MAX as usize) as u32,
            level_count: levels.len().min(u32::MAX as usize) as u32,
            levels,
        }
    }

    fn execution_dependencies(
        &self,
        pass_indices_by_id: &BTreeMap<u32, usize>,
    ) -> BTreeSet<(usize, usize)> {
        self.derived_barriers()
            .into_iter()
            .filter_map(|barrier| {
                let before = *pass_indices_by_id.get(&barrier.before_pass_id)?;
                let after = *pass_indices_by_id.get(&barrier.after_pass_id)?;
                (before != after).then_some((before, after))
            })
            .collect()
    }

    fn execution_levels_by_pass_index(
        &self,
        dependencies: &BTreeSet<(usize, usize)>,
        pass_indices_by_id: &BTreeMap<u32, usize>,
    ) -> Vec<u32> {
        let mut successors = vec![Vec::<usize>::new(); self.passes.len()];
        let mut indegrees = vec![0usize; self.passes.len()];
        for (before, after) in dependencies {
            successors[*before].push(*after);
            indegrees[*after] = indegrees[*after].saturating_add(1);
        }

        let mut levels = vec![0u32; self.passes.len()];
        let mut ready = pass_indices_by_id
            .values()
            .copied()
            .collect::<BTreeSet<_>>();
        ready.retain(|index| indegrees[*index] == 0);
        let mut visited = 0usize;
        while let Some(index) = ready.pop_first() {
            visited = visited.saturating_add(1);
            for successor in &successors[index] {
                levels[*successor] = levels[*successor].max(levels[index].saturating_add(1));
                indegrees[*successor] = indegrees[*successor].saturating_sub(1);
                if indegrees[*successor] == 0 {
                    ready.insert(*successor);
                }
            }
        }

        if visited == self.passes.len() {
            levels
        } else {
            (0..self.passes.len())
                .map(|index| index.min(u32::MAX as usize) as u32)
                .collect()
        }
    }
}

fn render_graph_execution_pass(pass: &RenderPassNode) -> RenderGraphExecutionPass {
    RenderGraphExecutionPass {
        pass_id: pass.id,
        role: pass.role,
        target: pass.target,
        target_name: pass.target_name.clone(),
        priority: render_graph_execution_priority(pass),
    }
}

fn render_graph_execution_priority(pass: &RenderPassNode) -> u32 {
    match pass.role {
        RenderPassRole::Clear => 1,
        RenderPassRole::BaseMaterial
        | RenderPassRole::EffectMaterial
        | RenderPassRole::ColorBlendPassthrough
        | RenderPassRole::CopyTarget
        | RenderPassRole::SwapTargetReferences
        | RenderPassRole::VideoSample
        | RenderPassRole::Particle
        | RenderPassRole::TextPath
        | RenderPassRole::SceneComposite => 3,
        RenderPassRole::DebugEvidence => 5,
        RenderPassRole::Unsupported => 7,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::SceneBlendMode;
    use crate::engine::render_graph::{
        CullMode, DepthTestMode, PassState, PipelineBlendMode, TextureBindingRole,
    };

    fn pass(
        id: u32,
        target: RenderTargetRole,
        target_name: Option<&str>,
        bindings: Vec<TextureBindingRole>,
    ) -> RenderPassNode {
        RenderPassNode {
            id,
            role: RenderPassRole::EffectMaterial,
            object_index: Some(7),
            material_index: None,
            pass_index: id,
            shader: None,
            target,
            target_name: target_name.map(str::to_owned),
            target_extent: Some([512, 256]),
            target_format: None,
            bindings,
            state: PassState {
                pipeline_blend: PipelineBlendMode::Normal,
                scene_blend: SceneBlendMode::Normal,
                shader_blend: None,
                depth_test: DepthTestMode::Disabled,
                depth_write: false,
                cull_mode: CullMode::None,
            },
        }
    }

    #[test]
    fn execution_plan_batches_independent_writes_before_joining_scene_pass() {
        let graph = RenderGraph {
            passes: vec![
                pass(0, RenderTargetRole::ImageLocalMain, Some("a"), vec![]),
                pass(1, RenderTargetRole::ImageLocalSub, Some("b"), vec![]),
                pass(
                    2,
                    RenderTargetRole::SceneColor,
                    None,
                    vec![
                        TextureBindingRole::GraphTarget {
                            slot: 0,
                            role: RenderTargetRole::ImageLocalMain,
                            name: Some("a".to_owned()),
                        },
                        TextureBindingRole::GraphTarget {
                            slot: 1,
                            role: RenderTargetRole::ImageLocalSub,
                            name: Some("b".to_owned()),
                        },
                    ],
                ),
            ],
            target_specs: Vec::new(),
            unsupported: Vec::new(),
        };

        let plan = graph.execution_plan();

        assert_eq!(plan.pass_count, 3);
        assert_eq!(plan.dependency_count, 2);
        assert_eq!(plan.level_count, 2);
        assert_eq!(
            plan.levels
                .iter()
                .map(|level| {
                    level
                        .passes
                        .iter()
                        .map(|pass| pass.pass_id)
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>(),
            vec![vec![0, 1], vec![2]]
        );
    }

    #[test]
    fn execution_plan_keeps_ping_pong_effect_targets_serialized() {
        let graph = RenderGraph {
            passes: vec![
                pass(0, RenderTargetRole::ImageLocalMain, Some("a"), vec![]),
                pass(
                    1,
                    RenderTargetRole::ImageLocalSub,
                    Some("b"),
                    vec![TextureBindingRole::GraphTarget {
                        slot: 0,
                        role: RenderTargetRole::ImageLocalMain,
                        name: Some("a".to_owned()),
                    }],
                ),
                pass(
                    2,
                    RenderTargetRole::ImageLocalMain,
                    Some("a"),
                    vec![TextureBindingRole::GraphTarget {
                        slot: 0,
                        role: RenderTargetRole::ImageLocalSub,
                        name: Some("b".to_owned()),
                    }],
                ),
            ],
            target_specs: Vec::new(),
            unsupported: Vec::new(),
        };

        let plan = graph.execution_plan();

        assert_eq!(plan.pass_count, 3);
        assert_eq!(plan.dependency_count, 2);
        assert_eq!(plan.level_count, 3);
        assert_eq!(
            plan.levels
                .iter()
                .map(|level| level.passes[0].pass_id)
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
    }
}
