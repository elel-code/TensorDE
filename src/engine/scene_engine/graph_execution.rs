//! Backend-independent scene graph execution and target barrier planning.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/rendering_device_graph.h`
//! - `references/godot/servers/rendering/rendering_device_graph.cpp`

use std::collections::BTreeMap;

use serde::Serialize;

use super::{SceneGraph, SceneGraphDrawFamilyPlan, SceneGraphPass, SceneGraphTarget};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneGraphExecutionPlan {
    pub pass_count: usize,
    pub target_count: usize,
    pub target_barrier_count: usize,
    pub draw_family_plan: SceneGraphDrawFamilyPlan,
    pub indexed_graphics_draw_count: usize,
    pub non_indexed_draw_count: usize,
    pub indexed_mesh_graphics_draw_count: usize,
    pub quad_draw_count: usize,
    pub particle_emitter_draw_count: usize,
    pub target_read_count: usize,
    pub swapchain_output_count: usize,
    pub passes: Vec<SceneGraphExecutionPass>,
    pub target_lifetimes: Vec<SceneGraphTargetLifetime>,
    pub target_barriers: Vec<SceneGraphTargetBarrier>,
    pub command_order: [&'static str; 5],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneGraphExecutionPass {
    pub pass_index: usize,
    pub name: String,
    pub input: Option<SceneGraphTarget>,
    pub output: SceneGraphTarget,
    pub draw_index_start: usize,
    pub draw_index_end: usize,
    pub draw_count: usize,
    pub indexed_graphics_draw_count: usize,
    pub non_indexed_draw_count: usize,
    pub indexed_mesh_graphics_draw_count: usize,
    pub quad_draw_count: usize,
    pub particle_emitter_draw_count: usize,
    pub target_reads: Vec<SceneGraphTarget>,
    pub target_writes: Vec<SceneGraphTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneGraphTargetLifetime {
    pub target: SceneGraphTarget,
    pub first_use_pass: usize,
    pub last_use_pass: usize,
    pub first_write_pass: Option<usize>,
    pub last_write_pass: Option<usize>,
    pub first_read_pass: Option<usize>,
    pub last_read_pass: Option<usize>,
    pub final_usage: SceneGraphTargetUsage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneGraphTargetBarrier {
    pub target: SceneGraphTarget,
    pub before_pass: usize,
    pub after_pass: usize,
    pub previous_usage: SceneGraphTargetUsage,
    pub next_usage: SceneGraphTargetUsage,
    pub reason: SceneGraphTargetBarrierReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SceneGraphTargetUsage {
    ShaderSampledRead,
    ColorAttachmentWrite,
    Present,
}

impl SceneGraphTargetUsage {
    const fn is_write(self) -> bool {
        matches!(self, Self::ColorAttachmentWrite)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SceneGraphTargetBarrierReason {
    ReadAfterWrite,
    WriteAfterRead,
    WriteAfterWrite,
}

impl SceneGraphExecutionPlan {
    pub fn from_graph(graph: &SceneGraph) -> Self {
        let draw_family_plan = SceneGraphDrawFamilyPlan::from_graph(graph);
        let mut passes = Vec::with_capacity(graph.passes.len());
        let mut target_lifetimes = BTreeMap::<SceneGraphTarget, SceneGraphTargetLifetime>::new();
        let mut last_use_by_target =
            BTreeMap::<SceneGraphTarget, (usize, SceneGraphTargetUsage)>::new();
        let mut target_barriers = Vec::new();
        let mut target_read_count = 0usize;
        let mut swapchain_output_count = 0usize;
        let mut draw_index_start = 0usize;

        for (pass_index, pass) in graph.passes.iter().enumerate() {
            let family_pass = &draw_family_plan.passes[pass_index];
            let execution_pass =
                scene_graph_execution_pass(pass_index, draw_index_start, pass, family_pass);
            draw_index_start = execution_pass.draw_index_end;
            target_read_count = target_read_count.saturating_add(execution_pass.target_reads.len());
            if pass.output == SceneGraphTarget::Swapchain {
                swapchain_output_count = swapchain_output_count.saturating_add(1);
            }

            for target in &execution_pass.target_reads {
                record_target_use(
                    &mut target_lifetimes,
                    &mut last_use_by_target,
                    &mut target_barriers,
                    *target,
                    pass_index,
                    SceneGraphTargetUsage::ShaderSampledRead,
                );
            }
            for target in &execution_pass.target_writes {
                record_target_use(
                    &mut target_lifetimes,
                    &mut last_use_by_target,
                    &mut target_barriers,
                    *target,
                    pass_index,
                    SceneGraphTargetUsage::ColorAttachmentWrite,
                );
            }
            passes.push(execution_pass);
        }

        let target_lifetimes = target_lifetimes.into_values().collect::<Vec<_>>();
        Self {
            pass_count: graph.passes.len(),
            target_count: target_lifetimes.len(),
            target_barrier_count: target_barriers.len(),
            indexed_graphics_draw_count: draw_family_plan.indexed_mesh_graphics_draw_count,
            non_indexed_draw_count: draw_family_plan.unsupported_runtime_draw_count(),
            indexed_mesh_graphics_draw_count: draw_family_plan.indexed_mesh_graphics_draw_count,
            quad_draw_count: draw_family_plan.quad_draw_count,
            particle_emitter_draw_count: draw_family_plan.particle_emitter_draw_count,
            target_read_count,
            swapchain_output_count,
            passes,
            target_lifetimes,
            target_barriers,
            draw_family_plan,
            command_order: [
                "classify_scene_graph_draw_families",
                "collect_scene_graph_pass_target_uses",
                "derive_scene_graph_target_lifetimes",
                "derive_scene_graph_target_barriers",
                "emit_scene_graph_execution_plan",
            ],
        }
    }
}

fn scene_graph_execution_pass(
    pass_index: usize,
    draw_index_start: usize,
    pass: &SceneGraphPass,
    family_pass: &super::SceneGraphPassDrawFamilyPlan,
) -> SceneGraphExecutionPass {
    SceneGraphExecutionPass {
        pass_index,
        name: pass.name.clone(),
        input: pass.input,
        output: pass.output,
        draw_index_start,
        draw_index_end: draw_index_start.saturating_add(pass.draws.len()),
        draw_count: pass.draws.len(),
        indexed_graphics_draw_count: family_pass.indexed_mesh_graphics_draw_count,
        non_indexed_draw_count: family_pass
            .quad_draw_count
            .saturating_add(family_pass.particle_emitter_draw_count),
        indexed_mesh_graphics_draw_count: family_pass.indexed_mesh_graphics_draw_count,
        quad_draw_count: family_pass.quad_draw_count,
        particle_emitter_draw_count: family_pass.particle_emitter_draw_count,
        target_reads: pass.input.into_iter().collect(),
        target_writes: vec![pass.output],
    }
}

fn record_target_use(
    lifetimes: &mut BTreeMap<SceneGraphTarget, SceneGraphTargetLifetime>,
    last_use_by_target: &mut BTreeMap<SceneGraphTarget, (usize, SceneGraphTargetUsage)>,
    barriers: &mut Vec<SceneGraphTargetBarrier>,
    target: SceneGraphTarget,
    pass_index: usize,
    usage: SceneGraphTargetUsage,
) {
    if let Some((before_pass, previous_usage)) = last_use_by_target.get(&target).copied()
        && before_pass != pass_index
        && target_usage_needs_barrier(previous_usage, usage)
    {
        barriers.push(SceneGraphTargetBarrier {
            target,
            before_pass,
            after_pass: pass_index,
            previous_usage,
            next_usage: usage,
            reason: target_barrier_reason(previous_usage, usage),
        });
    }

    let lifetime = lifetimes
        .entry(target)
        .or_insert_with(|| SceneGraphTargetLifetime {
            target,
            first_use_pass: pass_index,
            last_use_pass: pass_index,
            first_write_pass: None,
            last_write_pass: None,
            first_read_pass: None,
            last_read_pass: None,
            final_usage: scene_target_final_usage(target, usage),
        });
    lifetime.last_use_pass = pass_index;
    lifetime.final_usage = scene_target_final_usage(target, usage);
    if usage.is_write() {
        if lifetime.first_write_pass.is_none() {
            lifetime.first_write_pass = Some(pass_index);
        }
        lifetime.last_write_pass = Some(pass_index);
    } else {
        if lifetime.first_read_pass.is_none() {
            lifetime.first_read_pass = Some(pass_index);
        }
        lifetime.last_read_pass = Some(pass_index);
    }
    last_use_by_target.insert(target, (pass_index, usage));
}

fn target_usage_needs_barrier(
    previous: SceneGraphTargetUsage,
    next: SceneGraphTargetUsage,
) -> bool {
    previous.is_write() || next.is_write()
}

fn target_barrier_reason(
    previous: SceneGraphTargetUsage,
    next: SceneGraphTargetUsage,
) -> SceneGraphTargetBarrierReason {
    match (previous.is_write(), next.is_write()) {
        (true, false) => SceneGraphTargetBarrierReason::ReadAfterWrite,
        (false, true) => SceneGraphTargetBarrierReason::WriteAfterRead,
        (true, true) => SceneGraphTargetBarrierReason::WriteAfterWrite,
        (false, false) => SceneGraphTargetBarrierReason::ReadAfterWrite,
    }
}

fn scene_target_final_usage(
    target: SceneGraphTarget,
    usage: SceneGraphTargetUsage,
) -> SceneGraphTargetUsage {
    if target == SceneGraphTarget::Swapchain && usage.is_write() {
        SceneGraphTargetUsage::Present
    } else {
        usage
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneGeometryId, SceneGraphDraw, SceneGraphPass,
        SceneGraphPipelineClass, SceneMaterialKey, SceneObjectId,
    };

    #[test]
    fn execution_plan_tracks_swapchain_indexed_pass() {
        let graph = SceneGraph {
            passes: vec![pass(
                "scene-main",
                None,
                SceneGraphTarget::Swapchain,
                vec![mesh_draw(SceneObjectId(1))],
            )],
        };

        let plan = SceneGraphExecutionPlan::from_graph(&graph);

        assert_eq!(plan.pass_count, 1);
        assert_eq!(plan.target_count, 1);
        assert_eq!(plan.target_barrier_count, 0);
        assert_eq!(plan.indexed_graphics_draw_count, 1);
        assert_eq!(plan.non_indexed_draw_count, 0);
        assert_eq!(plan.target_read_count, 0);
        assert_eq!(
            plan.target_lifetimes[0].final_usage,
            SceneGraphTargetUsage::Present
        );
        assert_eq!(plan.passes[0].draw_index_start, 0);
        assert_eq!(plan.passes[0].draw_index_end, 1);
        assert_eq!(plan.passes[0].name, "scene-main");
    }

    #[test]
    fn execution_plan_derives_offscreen_read_after_write_barrier() {
        let graph = SceneGraph {
            passes: vec![
                pass(
                    "scene-offscreen",
                    None,
                    SceneGraphTarget::ImageLocalMain(0),
                    vec![mesh_draw(SceneObjectId(1))],
                ),
                pass(
                    "scene-main",
                    Some(SceneGraphTarget::ImageLocalMain(0)),
                    SceneGraphTarget::Swapchain,
                    vec![mesh_draw(SceneObjectId(2))],
                ),
            ],
        };

        let plan = SceneGraphExecutionPlan::from_graph(&graph);

        assert_eq!(plan.target_count, 2);
        assert_eq!(plan.target_barrier_count, 1);
        assert_eq!(plan.target_read_count, 1);
        assert_eq!(
            plan.target_barriers[0],
            SceneGraphTargetBarrier {
                target: SceneGraphTarget::ImageLocalMain(0),
                before_pass: 0,
                after_pass: 1,
                previous_usage: SceneGraphTargetUsage::ColorAttachmentWrite,
                next_usage: SceneGraphTargetUsage::ShaderSampledRead,
                reason: SceneGraphTargetBarrierReason::ReadAfterWrite,
            }
        );
    }

    #[test]
    fn execution_plan_keeps_ping_pong_target_barriers_ordered() {
        let graph = SceneGraph {
            passes: vec![
                pass(
                    "write-a",
                    None,
                    SceneGraphTarget::ImageLocalMain(0),
                    vec![mesh_draw(SceneObjectId(1))],
                ),
                pass(
                    "read-a-write-b",
                    Some(SceneGraphTarget::ImageLocalMain(0)),
                    SceneGraphTarget::ImageLocalSub(0),
                    vec![mesh_draw(SceneObjectId(2))],
                ),
                pass(
                    "read-b-write-a",
                    Some(SceneGraphTarget::ImageLocalSub(0)),
                    SceneGraphTarget::ImageLocalMain(0),
                    vec![mesh_draw(SceneObjectId(3))],
                ),
            ],
        };

        let plan = SceneGraphExecutionPlan::from_graph(&graph);

        assert_eq!(
            plan.target_barriers
                .iter()
                .map(|barrier| (
                    barrier.target,
                    barrier.before_pass,
                    barrier.after_pass,
                    barrier.reason
                ))
                .collect::<Vec<_>>(),
            vec![
                (
                    SceneGraphTarget::ImageLocalMain(0),
                    0,
                    1,
                    SceneGraphTargetBarrierReason::ReadAfterWrite
                ),
                (
                    SceneGraphTarget::ImageLocalSub(0),
                    1,
                    2,
                    SceneGraphTargetBarrierReason::ReadAfterWrite
                ),
                (
                    SceneGraphTarget::ImageLocalMain(0),
                    1,
                    2,
                    SceneGraphTargetBarrierReason::WriteAfterRead
                )
            ]
        );
        assert_eq!(
            plan.passes
                .iter()
                .map(|pass| (
                    pass.name.as_str(),
                    pass.draw_index_start,
                    pass.draw_index_end
                ))
                .collect::<Vec<_>>(),
            vec![
                ("write-a", 0, 1),
                ("read-a-write-b", 1, 2),
                ("read-b-write-a", 2, 3)
            ]
        );
    }

    #[test]
    fn execution_plan_rejects_single_swapchain_pass_with_offscreen_input() {
        let graph = SceneGraph {
            passes: vec![pass(
                "scene-main",
                Some(SceneGraphTarget::ImageLocalMain(0)),
                SceneGraphTarget::Swapchain,
                vec![mesh_draw(SceneObjectId(1))],
            )],
        };

        let plan = SceneGraphExecutionPlan::from_graph(&graph);

        assert_eq!(plan.target_read_count, 1);
        assert_eq!(plan.pass_count, 1);
        assert_eq!(
            plan.passes[0].target_reads,
            vec![SceneGraphTarget::ImageLocalMain(0)]
        );
    }

    #[test]
    fn execution_plan_counts_non_indexed_draws_without_downgrading_them() {
        let mut quad = mesh_draw(SceneObjectId(2));
        quad.pipeline = SceneGraphPipelineClass::Quad;
        quad.geometry = None;
        let mut particle = mesh_draw(SceneObjectId(3));
        particle.pipeline = SceneGraphPipelineClass::ParticleEmitter;
        particle.geometry = None;
        let graph = SceneGraph {
            passes: vec![pass(
                "scene-main",
                None,
                SceneGraphTarget::Swapchain,
                vec![mesh_draw(SceneObjectId(1)), quad, particle],
            )],
        };

        let plan = SceneGraphExecutionPlan::from_graph(&graph);

        assert_eq!(plan.indexed_graphics_draw_count, 1);
        assert_eq!(plan.indexed_mesh_graphics_draw_count, 1);
        assert_eq!(plan.quad_draw_count, 1);
        assert_eq!(plan.particle_emitter_draw_count, 1);
        assert_eq!(plan.non_indexed_draw_count, 2);
        assert_eq!(plan.draw_family_plan.unsupported_runtime_draw_count(), 2);
        assert_eq!(plan.passes[0].quad_draw_count, 1);
        assert_eq!(plan.passes[0].particle_emitter_draw_count, 1);
    }

    fn pass(
        name: &str,
        input: Option<SceneGraphTarget>,
        output: SceneGraphTarget,
        draws: Vec<SceneGraphDraw>,
    ) -> SceneGraphPass {
        SceneGraphPass {
            name: name.to_owned(),
            input,
            output,
            draws,
        }
    }

    fn mesh_draw(object: SceneObjectId) -> SceneGraphDraw {
        SceneGraphDraw {
            object,
            pipeline: SceneGraphPipelineClass::Mesh,
            material: SceneMaterialKey {
                shader: "we/genericimage4".to_owned(),
                blend: SceneBlendContract::TranslucentAlpha,
                render_state: crate::engine::scene_engine::SceneMaterialRenderState::translucent_2d(
                ),
            },
            geometry: Some(SceneGeometryId(object.0)),
            puppet: None,
            resources: vec![crate::engine::scene_engine::SceneGraphResourceBinding {
                slot: 0,
                role: crate::engine::scene_engine::SceneGraphResourceRole::shader_texture(0),
                resource: crate::engine::scene_engine::SceneResourceId(object.0),
            }],
            index_count: 6,
        }
    }
}
