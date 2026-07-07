//! Scene graph draw-family classification.
//!
//! References:
//! - `reverse-engineered/docs/scene-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/exe/blend-and-render.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`
//! - `references/godot/servers/rendering/rendering_device_graph.h`

use serde::Serialize;

use super::{SceneGraph, SceneGraphPipelineClass, SceneObjectId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub enum SceneGraphDrawFamily {
    IndexedMeshGraphics,
    Quad,
    LayerUtilityIndexed,
    ParticleEmitter,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneGraphDrawFamilyPlan {
    pub draw_count: usize,
    pub indexed_mesh_graphics_draw_count: usize,
    pub quad_draw_count: usize,
    pub layer_utility_indexed_draw_count: usize,
    pub particle_emitter_draw_count: usize,
    pub pass_count: usize,
    pub passes: Vec<SceneGraphPassDrawFamilyPlan>,
    pub command_order: [&'static str; 3],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SceneGraphPassDrawFamilyPlan {
    pub pass_index: usize,
    pub draw_index_start: usize,
    pub draw_index_end: usize,
    pub draw_count: usize,
    pub indexed_mesh_graphics_draw_count: usize,
    pub quad_draw_count: usize,
    pub layer_utility_indexed_draw_count: usize,
    pub particle_emitter_draw_count: usize,
    pub entries: Vec<SceneGraphDrawFamilyEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct SceneGraphDrawFamilyEntry {
    pub draw_index: usize,
    pub object: SceneObjectId,
    pub pipeline: SceneGraphPipelineClass,
    pub family: SceneGraphDrawFamily,
}

impl SceneGraphPipelineClass {
    pub const fn draw_family(self) -> SceneGraphDrawFamily {
        match self {
            Self::Mesh | Self::PuppetSkinning => SceneGraphDrawFamily::IndexedMeshGraphics,
            Self::Quad => SceneGraphDrawFamily::Quad,
            Self::LayerUtilityIndexed => SceneGraphDrawFamily::LayerUtilityIndexed,
            Self::ParticleEmitter => SceneGraphDrawFamily::ParticleEmitter,
        }
    }
}

impl SceneGraphDrawFamilyPlan {
    pub fn from_graph(graph: &SceneGraph) -> Self {
        let mut draw_index_start = 0usize;
        let mut indexed_mesh_graphics_draw_count = 0usize;
        let mut quad_draw_count = 0usize;
        let mut layer_utility_indexed_draw_count = 0usize;
        let mut particle_emitter_draw_count = 0usize;
        let passes = graph
            .passes
            .iter()
            .enumerate()
            .map(|(pass_index, pass)| {
                let plan =
                    SceneGraphPassDrawFamilyPlan::from_pass(pass_index, draw_index_start, pass);
                draw_index_start = plan.draw_index_end;
                indexed_mesh_graphics_draw_count = indexed_mesh_graphics_draw_count
                    .saturating_add(plan.indexed_mesh_graphics_draw_count);
                quad_draw_count = quad_draw_count.saturating_add(plan.quad_draw_count);
                layer_utility_indexed_draw_count = layer_utility_indexed_draw_count
                    .saturating_add(plan.layer_utility_indexed_draw_count);
                particle_emitter_draw_count =
                    particle_emitter_draw_count.saturating_add(plan.particle_emitter_draw_count);
                plan
            })
            .collect::<Vec<_>>();
        Self {
            draw_count: draw_index_start,
            indexed_mesh_graphics_draw_count,
            quad_draw_count,
            layer_utility_indexed_draw_count,
            particle_emitter_draw_count,
            pass_count: passes.len(),
            passes,
            command_order: [
                "classify_scene_graph_draw_families",
                "preserve_global_draw_indices",
                "emit_draw_family_plan",
            ],
        }
    }

    pub const fn unsupported_runtime_draw_count(&self) -> usize {
        self.quad_draw_count
            .saturating_add(self.layer_utility_indexed_draw_count)
            .saturating_add(self.particle_emitter_draw_count)
    }
}

impl SceneGraphPassDrawFamilyPlan {
    fn from_pass(pass_index: usize, draw_index_start: usize, pass: &super::SceneGraphPass) -> Self {
        let mut indexed_mesh_graphics_draw_count = 0usize;
        let mut quad_draw_count = 0usize;
        let mut layer_utility_indexed_draw_count = 0usize;
        let mut particle_emitter_draw_count = 0usize;
        let entries = pass
            .draws
            .iter()
            .enumerate()
            .map(|(local_draw_index, draw)| {
                let family = draw.pipeline.draw_family();
                match family {
                    SceneGraphDrawFamily::IndexedMeshGraphics => {
                        indexed_mesh_graphics_draw_count =
                            indexed_mesh_graphics_draw_count.saturating_add(1);
                    }
                    SceneGraphDrawFamily::Quad => {
                        quad_draw_count = quad_draw_count.saturating_add(1);
                    }
                    SceneGraphDrawFamily::LayerUtilityIndexed => {
                        layer_utility_indexed_draw_count =
                            layer_utility_indexed_draw_count.saturating_add(1);
                    }
                    SceneGraphDrawFamily::ParticleEmitter => {
                        particle_emitter_draw_count = particle_emitter_draw_count.saturating_add(1);
                    }
                }
                SceneGraphDrawFamilyEntry {
                    draw_index: draw_index_start.saturating_add(local_draw_index),
                    object: draw.object,
                    pipeline: draw.pipeline,
                    family,
                }
            })
            .collect::<Vec<_>>();
        Self {
            pass_index,
            draw_index_start,
            draw_index_end: draw_index_start.saturating_add(pass.draws.len()),
            draw_count: pass.draws.len(),
            indexed_mesh_graphics_draw_count,
            quad_draw_count,
            layer_utility_indexed_draw_count,
            particle_emitter_draw_count,
            entries,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneGraphDraw, SceneGraphPass, SceneGraphResourceBinding,
        SceneGraphResourceRole, SceneGraphTarget, SceneMaterialKey, SceneResourceId,
    };

    #[test]
    fn draw_family_plan_preserves_global_draw_indices_across_passes() {
        let graph = SceneGraph {
            passes: vec![
                pass(vec![
                    draw(SceneObjectId(1), SceneGraphPipelineClass::Mesh),
                    draw(SceneObjectId(2), SceneGraphPipelineClass::Quad),
                ]),
                pass(vec![
                    draw(SceneObjectId(3), SceneGraphPipelineClass::PuppetSkinning),
                    draw(
                        SceneObjectId(4),
                        SceneGraphPipelineClass::LayerUtilityIndexed,
                    ),
                    draw(SceneObjectId(5), SceneGraphPipelineClass::ParticleEmitter),
                ]),
            ],
        };

        let plan = SceneGraphDrawFamilyPlan::from_graph(&graph);

        assert_eq!(plan.draw_count, 5);
        assert_eq!(plan.indexed_mesh_graphics_draw_count, 2);
        assert_eq!(plan.quad_draw_count, 1);
        assert_eq!(plan.layer_utility_indexed_draw_count, 1);
        assert_eq!(plan.particle_emitter_draw_count, 1);
        assert_eq!(plan.unsupported_runtime_draw_count(), 3);
        assert_eq!(
            plan.passes
                .iter()
                .flat_map(|pass| pass.entries.iter())
                .map(|entry| (entry.draw_index, entry.object, entry.family))
                .collect::<Vec<_>>(),
            vec![
                (
                    0,
                    SceneObjectId(1),
                    SceneGraphDrawFamily::IndexedMeshGraphics
                ),
                (1, SceneObjectId(2), SceneGraphDrawFamily::Quad),
                (
                    2,
                    SceneObjectId(3),
                    SceneGraphDrawFamily::IndexedMeshGraphics
                ),
                (
                    3,
                    SceneObjectId(4),
                    SceneGraphDrawFamily::LayerUtilityIndexed
                ),
                (4, SceneObjectId(5), SceneGraphDrawFamily::ParticleEmitter),
            ]
        );
    }

    fn pass(draws: Vec<SceneGraphDraw>) -> SceneGraphPass {
        SceneGraphPass {
            name: "scene-main".to_owned(),
            input: None,
            output: SceneGraphTarget::Swapchain,
            draws,
        }
    }

    fn draw(object: SceneObjectId, pipeline: SceneGraphPipelineClass) -> SceneGraphDraw {
        SceneGraphDraw {
            object,
            pipeline,
            material: SceneMaterialKey {
                shader: "we/genericimage4".to_owned(),
                blend: SceneBlendContract::TranslucentAlpha,
                render_state: crate::engine::scene_engine::SceneMaterialRenderState::translucent_2d(
                ),
            },
            geometry: None,
            puppet: None,
            resources: vec![SceneGraphResourceBinding {
                slot: 0,
                role: SceneGraphResourceRole::shader_texture(0),
                resource: SceneResourceId(object.0),
            }],
            index_count: 6,
        }
    }
}
