use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use super::execution::{RenderGraphExecutionPass, RenderGraphExecutionPlan};
use super::graph::RenderGraph;
use super::pass::{RenderPassNode, RenderPassRole};
use super::target::RenderTargetRole;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderGraphRunPlan {
    pub selected_schedule: String,
    pub selected_physical_target_count: u32,
    pub candidates: Vec<RenderGraphRunPlanCandidate>,
    pub pass_count: u32,
    pub target_run_count: u32,
    pub scene_color_run_count: u32,
    pub offscreen_target_run_count: u32,
    pub repeated_target_run_count: u32,
    pub max_target_run_count: u32,
    pub max_run_pass_count: u32,
    pub runs: Vec<RenderGraphTargetRun>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderGraphRunPlanCandidate {
    pub schedule: String,
    pub physical_target_count: u32,
    pub target_run_count: u32,
    pub scene_color_run_count: u32,
    pub offscreen_target_run_count: u32,
    pub repeated_target_run_count: u32,
    pub max_target_run_count: u32,
    pub max_run_pass_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderGraphTargetRun {
    pub run_index: u32,
    pub first_level: u32,
    pub last_level: u32,
    pub target: RenderTargetRole,
    pub target_name: Option<String>,
    pub pass_count: u32,
    pub passes: Vec<RenderGraphTargetRunPass>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenderGraphTargetRunPass {
    pub level: u32,
    pub pass_id: u32,
    pub role: RenderPassRole,
    pub priority: u32,
}

impl RenderGraph {
    pub fn run_plan(&self) -> RenderGraphRunPlan {
        self.run_plan_from_execution(&self.execution_plan())
    }

    pub fn run_plan_from_execution(
        &self,
        execution: &RenderGraphExecutionPlan,
    ) -> RenderGraphRunPlan {
        if execution.pass_count == 0 {
            return RenderGraphRunPlan::default();
        }

        let candidates = vec![
            (
                RenderGraphRunSchedule::TargetAffine,
                self.render_graph_run_plan_from_ordered_passes(
                    execution.pass_count,
                    self.render_graph_dependency_execution_passes(
                        execution,
                        RenderGraphRunSchedule::TargetAffine,
                    ),
                ),
            ),
            (
                RenderGraphRunSchedule::LevelIndex,
                self.render_graph_run_plan_from_ordered_passes(
                    execution.pass_count,
                    self.render_graph_dependency_execution_passes(
                        execution,
                        RenderGraphRunSchedule::LevelIndex,
                    ),
                ),
            ),
            (
                RenderGraphRunSchedule::TargetLocal,
                self.render_graph_run_plan_from_ordered_passes(
                    execution.pass_count,
                    render_graph_target_local_execution_passes(execution),
                ),
            ),
        ];
        let candidate_summaries = candidates
            .iter()
            .map(|(schedule, plan)| self.render_graph_run_plan_candidate(*schedule, plan))
            .collect::<Vec<_>>();
        let Some((selected_schedule, mut selected_plan)) = candidates
            .into_iter()
            .min_by_key(|(_, plan)| self.render_graph_run_plan_score(plan))
        else {
            return RenderGraphRunPlan::default();
        };
        selected_plan.selected_schedule = selected_schedule.label().to_owned();
        selected_plan.selected_physical_target_count = self
            .target_allocation_plan_for_run_plan(&selected_plan)
            .physical_target_count;
        selected_plan.candidates = candidate_summaries;
        selected_plan
    }

    fn render_graph_run_plan_score(&self, plan: &RenderGraphRunPlan) -> (u32, u32, u32) {
        let allocation = self.target_allocation_plan_for_run_plan(plan);
        (
            allocation.physical_target_count,
            plan.target_run_count,
            plan.repeated_target_run_count,
        )
    }

    fn render_graph_run_plan_candidate(
        &self,
        schedule: RenderGraphRunSchedule,
        plan: &RenderGraphRunPlan,
    ) -> RenderGraphRunPlanCandidate {
        RenderGraphRunPlanCandidate {
            schedule: schedule.label().to_owned(),
            physical_target_count: self
                .target_allocation_plan_for_run_plan(plan)
                .physical_target_count,
            target_run_count: plan.target_run_count,
            scene_color_run_count: plan.scene_color_run_count,
            offscreen_target_run_count: plan.offscreen_target_run_count,
            repeated_target_run_count: plan.repeated_target_run_count,
            max_target_run_count: plan.max_target_run_count,
            max_run_pass_count: plan.max_run_pass_count,
        }
    }

    fn render_graph_run_plan_from_ordered_passes(
        &self,
        pass_count: u32,
        ordered_passes: Vec<(u32, RenderGraphExecutionPass)>,
    ) -> RenderGraphRunPlan {
        let mut runs = Vec::<RenderGraphTargetRun>::new();
        for (level, pass) in ordered_passes {
            let same_target = runs.last().is_some_and(|run| {
                run.target == pass.target && run.target_name == pass.target_name
            });
            if same_target {
                let run = runs.last_mut().expect("same_target requires last run");
                run.last_level = level;
                run.pass_count = run.pass_count.saturating_add(1);
                run.passes.push(render_graph_target_run_pass(level, &pass));
                continue;
            }

            let run_index = runs.len().min(u32::MAX as usize) as u32;
            runs.push(RenderGraphTargetRun {
                run_index,
                first_level: level,
                last_level: level,
                target: pass.target,
                target_name: pass.target_name.clone(),
                pass_count: 1,
                passes: vec![render_graph_target_run_pass(level, &pass)],
            });
        }

        let mut target_run_counts = BTreeMap::<String, u32>::new();
        let mut scene_color_run_count = 0u32;
        let mut offscreen_target_run_count = 0u32;
        let mut max_run_pass_count = 0u32;
        for run in &runs {
            max_run_pass_count = max_run_pass_count.max(run.pass_count);
            if render_graph_target_run_is_scene_color(run) {
                scene_color_run_count = scene_color_run_count.saturating_add(1);
            } else {
                offscreen_target_run_count = offscreen_target_run_count.saturating_add(1);
            }
            *target_run_counts
                .entry(render_graph_target_run_key(run))
                .or_default() += 1;
        }
        let repeated_target_run_count = target_run_counts
            .values()
            .map(|count| count.saturating_sub(1))
            .sum::<u32>();
        let max_target_run_count = target_run_counts
            .values()
            .copied()
            .max()
            .unwrap_or_default();

        RenderGraphRunPlan {
            pass_count,
            target_run_count: runs.len().min(u32::MAX as usize) as u32,
            scene_color_run_count,
            offscreen_target_run_count,
            repeated_target_run_count,
            max_target_run_count,
            max_run_pass_count,
            runs,
            ..RenderGraphRunPlan::default()
        }
    }

    fn render_graph_dependency_execution_passes(
        &self,
        execution: &RenderGraphExecutionPlan,
        schedule: RenderGraphRunSchedule,
    ) -> Vec<(u32, RenderGraphExecutionPass)> {
        let pass_indices_by_id = self
            .passes
            .iter()
            .enumerate()
            .map(|(index, pass)| (pass.id, index))
            .collect::<BTreeMap<_, _>>();
        if pass_indices_by_id.len() != self.passes.len() {
            return render_graph_target_local_execution_passes(execution);
        }

        let dependencies = self
            .derived_barriers()
            .into_iter()
            .filter_map(|barrier| {
                let before = *pass_indices_by_id.get(&barrier.before_pass_id)?;
                let after = *pass_indices_by_id.get(&barrier.after_pass_id)?;
                (before != after).then_some((before, after))
            })
            .collect::<BTreeSet<_>>();
        let mut successors = vec![Vec::<usize>::new(); self.passes.len()];
        let mut indegrees = vec![0usize; self.passes.len()];
        for (before, after) in dependencies {
            successors[before].push(after);
            indegrees[after] = indegrees[after].saturating_add(1);
        }

        let execution_passes_by_id = render_graph_execution_passes_by_id(execution);
        let mut ready = (0..self.passes.len())
            .filter(|index| indegrees[*index] == 0)
            .collect::<BTreeSet<_>>();
        let mut ordered = Vec::<(u32, RenderGraphExecutionPass)>::new();
        let mut last_target = None::<RenderGraphRunTargetKey>;

        while let Some(index) = render_graph_select_ready_pass(
            &ready,
            &self.passes,
            &execution_passes_by_id,
            schedule,
            last_target.as_ref(),
        ) {
            ready.remove(&index);
            let pass = &self.passes[index];
            let Some((level, execution_pass)) = execution_passes_by_id.get(&pass.id).cloned()
            else {
                return render_graph_target_local_execution_passes(execution);
            };
            last_target = Some(RenderGraphRunTargetKey {
                target: execution_pass.target,
                target_name: execution_pass.target_name.clone(),
            });
            ordered.push((level, execution_pass));

            for successor in &successors[index] {
                indegrees[*successor] = indegrees[*successor].saturating_sub(1);
                if indegrees[*successor] == 0 {
                    ready.insert(*successor);
                }
            }
        }

        if ordered.len() == self.passes.len() {
            ordered
        } else {
            render_graph_target_local_execution_passes(execution)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RenderGraphRunTargetKey {
    target: RenderTargetRole,
    target_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RenderGraphRunSchedule {
    TargetAffine,
    LevelIndex,
    TargetLocal,
}

impl RenderGraphRunSchedule {
    fn label(self) -> &'static str {
        match self {
            Self::TargetAffine => "target-affine",
            Self::LevelIndex => "level-index",
            Self::TargetLocal => "target-local",
        }
    }
}

fn render_graph_target_local_execution_passes(
    execution: &RenderGraphExecutionPlan,
) -> Vec<(u32, RenderGraphExecutionPass)> {
    let mut ordered = Vec::new();
    for level in &execution.levels {
        let mut passes = level.passes.clone();
        passes.sort_by(|left, right| {
            left.priority
                .cmp(&right.priority)
                .then(
                    render_graph_execution_target_key(left)
                        .cmp(&render_graph_execution_target_key(right)),
                )
                .then(left.pass_id.cmp(&right.pass_id))
        });
        ordered.extend(passes.into_iter().map(|pass| (level.level, pass)));
    }
    ordered
}

fn render_graph_execution_passes_by_id(
    execution: &RenderGraphExecutionPlan,
) -> BTreeMap<u32, (u32, RenderGraphExecutionPass)> {
    execution
        .levels
        .iter()
        .flat_map(|level| {
            level
                .passes
                .iter()
                .cloned()
                .map(|pass| (pass.pass_id, (level.level, pass)))
        })
        .collect()
}

fn render_graph_select_ready_pass(
    ready: &BTreeSet<usize>,
    passes: &[RenderPassNode],
    execution_passes_by_id: &BTreeMap<u32, (u32, RenderGraphExecutionPass)>,
    schedule: RenderGraphRunSchedule,
    last_target: Option<&RenderGraphRunTargetKey>,
) -> Option<usize> {
    let mut best = None::<usize>;
    for index in ready {
        let pass = &passes[*index];
        if schedule == RenderGraphRunSchedule::LevelIndex {
            let earliest_level = ready
                .iter()
                .filter_map(|ready_index| {
                    execution_passes_by_id
                        .get(&passes[*ready_index].id)
                        .map(|(level, _)| *level)
                })
                .min()?;
            if execution_passes_by_id
                .get(&pass.id)
                .map(|(level, _)| *level)
                != Some(earliest_level)
            {
                continue;
            }
        }
        best = render_graph_better_ready_pass(best, *index, passes, schedule, last_target);
    }
    best
}

fn render_graph_better_ready_pass(
    current: Option<usize>,
    candidate: usize,
    passes: &[RenderPassNode],
    schedule: RenderGraphRunSchedule,
    last_target: Option<&RenderGraphRunTargetKey>,
) -> Option<usize> {
    let Some(current) = current else {
        return Some(candidate);
    };

    if render_graph_ready_pass_order_key(&passes[candidate], candidate, schedule, last_target)
        < render_graph_ready_pass_order_key(&passes[current], current, schedule, last_target)
    {
        Some(candidate)
    } else {
        Some(current)
    }
}

fn render_graph_ready_pass_order_key(
    pass: &RenderPassNode,
    index: usize,
    schedule: RenderGraphRunSchedule,
    last_target: Option<&RenderGraphRunTargetKey>,
) -> (u32, u32, String, u32, u32) {
    let same_target_rank = if schedule == RenderGraphRunSchedule::TargetAffine
        && last_target.is_some_and(|target| {
            target.target == pass.target && target.target_name == pass.target_name
        }) {
        0
    } else {
        1
    };
    let target_key = if schedule == RenderGraphRunSchedule::TargetAffine {
        render_graph_pass_target_key(pass)
    } else {
        String::new()
    };
    let index_key = index.min(u32::MAX as usize) as u32;
    (
        render_graph_pass_priority(pass),
        same_target_rank,
        target_key,
        index_key,
        pass.id,
    )
}

fn render_graph_pass_priority(pass: &RenderPassNode) -> u32 {
    match pass.role {
        RenderPassRole::Clear => 1,
        RenderPassRole::BaseMaterial
        | RenderPassRole::ObjectLocalSource
        | RenderPassRole::EffectMaterial
        | RenderPassRole::ColorBlendPassthrough
        | RenderPassRole::CopyTarget
        | RenderPassRole::SwapTargetReferences
        | RenderPassRole::VideoSample
        | RenderPassRole::Particle
        | RenderPassRole::TextPath
        | RenderPassRole::SceneComposite
        | RenderPassRole::MeshVisiblePrefix
        | RenderPassRole::MeshClippingMask
        | RenderPassRole::MeshClippedTarget
        | RenderPassRole::MeshVisibleRemainder => 3,
        RenderPassRole::DebugEvidence => 5,
        RenderPassRole::Unsupported => 7,
    }
}

fn render_graph_pass_target_key(pass: &RenderPassNode) -> String {
    format!("{:?}:{:?}", pass.target, pass.target_name)
}

fn render_graph_target_run_pass(
    level: u32,
    pass: &RenderGraphExecutionPass,
) -> RenderGraphTargetRunPass {
    RenderGraphTargetRunPass {
        level,
        pass_id: pass.pass_id,
        role: pass.role,
        priority: pass.priority,
    }
}

fn render_graph_target_run_is_scene_color(run: &RenderGraphTargetRun) -> bool {
    matches!(
        run.target,
        RenderTargetRole::SceneColor | RenderTargetRole::Swapchain
    )
}

fn render_graph_target_run_key(run: &RenderGraphTargetRun) -> String {
    format!("{:?}:{:?}", run.target, run.target_name)
}

fn render_graph_execution_target_key(pass: &RenderGraphExecutionPass) -> String {
    format!("{:?}:{:?}", pass.target, pass.target_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::SceneBlendMode;
    use crate::engine::render_graph::{
        CullMode, DepthTestMode, PassState, PipelineBlendMode, RenderPassDrawPrimitive,
        RenderPassNode, TextureBindingRole,
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
            draw_primitive: RenderPassDrawPrimitive::FullscreenTriangle,
            object_index: Some(7),
            material_index: None,
            pass_index: id,
            shader: None,
            target,
            target_name: target_name.map(str::to_owned),
            target_extent: Some([512, 256]),
            target_format: None,
            bindings,
            effect_visibility: crate::engine::render_graph::RenderPassEffectVisibility::NONE,
            state: PassState {
                pipeline_blend: PipelineBlendMode::Normal,
                scene_blend: SceneBlendMode::Normal,
                shader_blend: None,
                depth_test: DepthTestMode::Disabled,
                depth_write: false,
                cull_mode: CullMode::None,
                ..PassState::default()
            },
        }
    }

    #[test]
    fn run_plan_keeps_same_target_writes_together_when_lifetime_pressure_is_equal() {
        let graph = RenderGraph {
            activation_policy: Default::default(),
            passes: vec![
                pass(0, RenderTargetRole::ImageLocalMain, Some("a"), vec![]),
                pass(1, RenderTargetRole::ImageLocalSub, Some("b"), vec![]),
                pass(2, RenderTargetRole::ImageLocalMain, Some("a"), vec![]),
                pass(
                    3,
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

        let plan = graph.run_plan();

        assert_eq!(plan.pass_count, 4);
        assert_eq!(plan.candidates.len(), 3);
        assert!(plan.selected_physical_target_count > 0);
        assert!(
            plan.candidates
                .iter()
                .any(|candidate| candidate.schedule == "target-affine")
        );
        assert!(
            plan.candidates
                .iter()
                .any(|candidate| candidate.schedule == "level-index")
        );
        assert!(
            plan.candidates
                .iter()
                .any(|candidate| candidate.schedule == "target-local")
        );
        assert_eq!(plan.target_run_count, 3);
        assert_eq!(plan.offscreen_target_run_count, 2);
        assert_eq!(plan.scene_color_run_count, 1);
        assert_eq!(plan.repeated_target_run_count, 0);
        assert_eq!(plan.max_target_run_count, 1);
        assert_eq!(plan.max_run_pass_count, 2);
        assert_eq!(
            plan.runs[0]
                .passes
                .iter()
                .map(|pass| pass.pass_id)
                .collect::<Vec<_>>(),
            vec![0, 2]
        );
    }

    #[test]
    fn run_plan_prefers_fewer_target_runs_when_lifetime_pressure_is_equal() {
        let graph = RenderGraph {
            activation_policy: Default::default(),
            passes: vec![
                pass(0, RenderTargetRole::ImageLocalMain, Some("a"), vec![]),
                pass(
                    1,
                    RenderTargetRole::ImageLocalMain,
                    Some("a"),
                    vec![TextureBindingRole::GraphTarget {
                        slot: 0,
                        role: RenderTargetRole::ImageLocalMain,
                        name: Some("a".to_owned()),
                    }],
                ),
                pass(2, RenderTargetRole::SceneColor, None, vec![]),
            ],
            target_specs: Vec::new(),
            unsupported: Vec::new(),
        };

        let plan = graph.run_plan();

        assert_eq!(
            plan.runs
                .iter()
                .flat_map(|run| run.passes.iter().map(|pass| pass.pass_id))
                .collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert_eq!(plan.target_run_count, 2);
        assert_eq!(plan.repeated_target_run_count, 0);
    }

    #[test]
    fn run_plan_keeps_ping_pong_targets_as_repeated_runs() {
        let graph = RenderGraph {
            activation_policy: Default::default(),
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

        let plan = graph.run_plan();

        assert_eq!(plan.target_run_count, 3);
        assert_eq!(plan.repeated_target_run_count, 1);
        assert_eq!(plan.max_target_run_count, 2);
        assert_eq!(
            plan.runs
                .iter()
                .map(|run| (run.target, run.target_name.as_deref()))
                .collect::<Vec<_>>(),
            vec![
                (RenderTargetRole::ImageLocalMain, Some("a")),
                (RenderTargetRole::ImageLocalSub, Some("b")),
                (RenderTargetRole::ImageLocalMain, Some("a")),
            ]
        );
    }
}
