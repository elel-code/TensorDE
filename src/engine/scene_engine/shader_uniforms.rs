//! WE shader uniform frame plans.
//!
//! References:
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/shader-conventions.md`
//! - `reverse-engineered/docs/exe/global-uniforms.md`
//! - `reverse-engineered/shaders/genericimage4.frag`
//! - `references/godot/servers/rendering/rendering_device.h`

use serde::Serialize;

use super::{
    SCENE_GPU_GENERICIMAGE4_MATERIAL_UNIFORM_BYTES, SceneGraph, SceneObjectId, WeShaderInterface,
    WeVec4,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SceneShaderUniformFramePlan {
    pub draw_count: usize,
    pub genericimage4_material_record_count: usize,
    pub genericimage4_material_records: Vec<SceneGenericImage4MaterialUniformRecord>,
    pub record_bytes: u64,
    pub command_order: [&'static str; 3],
}

impl SceneShaderUniformFramePlan {
    pub fn from_graph(graph: &SceneGraph) -> Result<Self, String> {
        let mut draw_count = 0usize;
        let mut genericimage4_material_records = Vec::new();
        for pass in &graph.passes {
            for draw in &pass.draws {
                draw_count += 1;
                let interface =
                    WeShaderInterface::for_shader(&draw.material.shader).ok_or_else(|| {
                        format!(
                            "scene shader uniform plan references unknown WE shader '{}'",
                            draw.material.shader
                        )
                    })?;
                let texture_slot_mask =
                    draw.shader_texture_slot_mask_with_pass_input(pass.input)?;
                if interface.shader == "we/genericimage4" {
                    genericimage4_material_records.push(
                        SceneGenericImage4MaterialUniformRecord::from_draw(
                            genericimage4_material_records.len(),
                            draw.object,
                            draw.material.shader.clone(),
                            texture_slot_mask,
                        ),
                    );
                }
            }
        }

        Ok(Self {
            draw_count,
            genericimage4_material_record_count: genericimage4_material_records.len(),
            genericimage4_material_records,
            record_bytes: SCENE_GPU_GENERICIMAGE4_MATERIAL_UNIFORM_BYTES,
            command_order: [
                "validate_we_shader_uniform_interface",
                "collect_genericimage4_material_records",
                "upload_material_uniform_buffer",
            ],
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SceneGenericImage4MaterialUniformRecord {
    pub record_index: usize,
    pub object: SceneObjectId,
    pub shader: String,
    pub texture_slot_mask: u32,
    pub color4: WeVec4,
    pub roughness: f32,
    pub metallic: f32,
    pub specular_tint: [f32; 3],
}

impl SceneGenericImage4MaterialUniformRecord {
    fn from_draw(
        record_index: usize,
        object: SceneObjectId,
        shader: String,
        texture_slot_mask: u32,
    ) -> Self {
        Self {
            record_index,
            object,
            shader,
            texture_slot_mask,
            color4: WeVec4::ONE,
            roughness: 0.7,
            metallic: 0.0,
            specular_tint: [1.0, 1.0, 1.0],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneBlendContract, SceneGeometryId, SceneGraphDraw, SceneGraphPass,
        SceneGraphPipelineClass, SceneGraphResourceBinding, SceneGraphResourceRole,
        SceneGraphTarget, SceneMaterialKey, SceneMaterialRenderState, SceneResourceId,
    };

    #[test]
    fn shader_uniform_plan_collects_genericimage4_material_records() {
        let graph = SceneGraph {
            passes: vec![SceneGraphPass {
                name: "scene-main".to_owned(),
                input: None,
                output: SceneGraphTarget::Swapchain,
                draws: vec![genericimage4_draw(SceneObjectId(7), SceneResourceId(3))],
            }],
        };

        let plan = SceneShaderUniformFramePlan::from_graph(&graph).unwrap();

        assert_eq!(plan.draw_count, 1);
        assert_eq!(plan.genericimage4_material_record_count, 1);
        assert_eq!(
            plan.record_bytes,
            SCENE_GPU_GENERICIMAGE4_MATERIAL_UNIFORM_BYTES
        );
        let record = &plan.genericimage4_material_records[0];
        assert_eq!(record.object, SceneObjectId(7));
        assert_eq!(record.texture_slot_mask, 1);
        assert_eq!(record.color4.lanes(), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(record.roughness, 0.7);
        assert_eq!(record.metallic, 0.0);
        assert_eq!(record.specular_tint, [1.0, 1.0, 1.0]);
    }

    #[test]
    fn shader_uniform_plan_rejects_missing_genericimage4_texture0() {
        let mut draw = genericimage4_draw(SceneObjectId(7), SceneResourceId(3));
        draw.resources.clear();
        let graph = SceneGraph {
            passes: vec![SceneGraphPass {
                name: "scene-main".to_owned(),
                input: None,
                output: SceneGraphTarget::Swapchain,
                draws: vec![draw],
            }],
        };

        let err = SceneShaderUniformFramePlan::from_graph(&graph)
            .expect_err("genericimage4 requires g_Texture0");

        assert!(err.contains("requires texture slots"));
    }

    #[test]
    fn shader_uniform_plan_uses_pass_input_as_genericimage4_texture0() {
        let mut draw = genericimage4_draw(SceneObjectId(7), SceneResourceId(3));
        draw.resources.clear();
        let graph = SceneGraph {
            passes: vec![SceneGraphPass {
                name: "effect-resolve".to_owned(),
                input: Some(SceneGraphTarget::EffectTarget(0)),
                output: SceneGraphTarget::Swapchain,
                draws: vec![draw],
            }],
        };

        let plan = SceneShaderUniformFramePlan::from_graph(&graph).unwrap();

        assert_eq!(plan.genericimage4_material_records[0].texture_slot_mask, 1);
    }

    #[test]
    fn shader_uniform_plan_rejects_pass_input_texture0_collision() {
        let graph = SceneGraph {
            passes: vec![SceneGraphPass {
                name: "effect-resolve".to_owned(),
                input: Some(SceneGraphTarget::EffectTarget(0)),
                output: SceneGraphTarget::Swapchain,
                draws: vec![genericimage4_draw(SceneObjectId(7), SceneResourceId(3))],
            }],
        };

        let err = SceneShaderUniformFramePlan::from_graph(&graph)
            .expect_err("pass input and draw texture0 collide");

        assert!(err.contains("collides with WE g_Texture0"));
    }

    fn genericimage4_draw(object: SceneObjectId, texture: SceneResourceId) -> SceneGraphDraw {
        SceneGraphDraw {
            object,
            pipeline: SceneGraphPipelineClass::Mesh,
            material: SceneMaterialKey {
                shader: "we/genericimage4".to_owned(),
                blend: SceneBlendContract::TranslucentAlpha,
                render_state: SceneMaterialRenderState::translucent_2d(),
            },
            geometry: Some(SceneGeometryId(object.0)),
            puppet: None,
            resources: vec![SceneGraphResourceBinding {
                slot: 0,
                role: SceneGraphResourceRole::shader_texture(0),
                resource: texture,
            }],
            index_count: 6,
        }
    }
}
