//! WE effect uniform frame plans.
//!
//! References:
//! - `reverse-engineered/effects/iris.md`
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/shader-conventions.md`
//! - `reverse-engineered/shaders/effects/iris.vert`
//! - `reverse-engineered/shaders/effects/iris.frag`
//! - `references/godot/servers/rendering/rendering_device_graph.h`

use serde::Serialize;

use super::{
    SceneEffectConstantValue, SceneEffectPassGraphMaterialPass, SceneEffectPassGraphPlan,
    SceneFrameContext, SceneObjectId,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SceneEffectUniformFramePlan {
    pub effect_pass_count: usize,
    pub iris_record_count: usize,
    pub iris_records: Vec<SceneIrisEffectUniformRecord>,
    pub command_order: [&'static str; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SceneIrisEffectUniformRecord {
    pub record_index: usize,
    pub effect_pass_index: usize,
    pub object: SceneObjectId,
    pub pass_index: usize,
    pub shader: String,
    pub time_seconds: f32,
    pub texture_slot_mask: u32,
    pub texture_resolution_slots: Vec<u32>,
    pub scale: [f32; 2],
    pub speed: f32,
    pub rough: f32,
    pub noise_amount: f32,
    pub phase_offset: f32,
    pub eye_color: [f32; 3],
    pub mask_combo: u32,
    pub background_combo: u32,
}

impl SceneEffectUniformFramePlan {
    pub fn empty() -> Self {
        Self {
            effect_pass_count: 0,
            iris_record_count: 0,
            iris_records: Vec::new(),
            command_order: [
                "scan_effect_material_pass_uniform_contracts",
                "lower_iris_material_constants",
                "resolve_iris_combo_uniform_requirements",
                "emit_effect_uniform_frame_plan",
            ],
        }
    }

    pub fn from_effect_pass_graph(
        context: SceneFrameContext,
        graph: &SceneEffectPassGraphPlan,
    ) -> Result<Self, String> {
        let mut plan = Self::empty();
        plan.effect_pass_count = graph.passes.len();
        for pass in &graph.passes {
            if pass.shader.as_deref() == Some("effects/iris") {
                plan.iris_records
                    .push(SceneIrisEffectUniformRecord::from_pass(
                        plan.iris_records.len(),
                        context,
                        pass,
                    )?);
            }
        }
        plan.iris_record_count = plan.iris_records.len();
        Ok(plan)
    }
}

impl SceneIrisEffectUniformRecord {
    fn from_pass(
        record_index: usize,
        context: SceneFrameContext,
        pass: &SceneEffectPassGraphMaterialPass,
    ) -> Result<Self, String> {
        let mask_combo = effect_combo_u32(pass, "MASK")?;
        let background_combo = effect_combo_u32(pass, "BACKGROUND")?;
        let texture_resolution_slots = if mask_combo != 0 { vec![1] } else { Vec::new() };
        Ok(Self {
            record_index,
            effect_pass_index: pass.graph_pass_index,
            object: pass.object,
            pass_index: pass.pass_index,
            shader: "effects/iris".to_owned(),
            time_seconds: context.time_ms as f32 / 1000.0,
            texture_slot_mask: effect_pass_texture_slot_mask(pass)?,
            texture_resolution_slots,
            scale: effect_vec2_constant(pass, "scale", [1.0, 1.0])?,
            speed: effect_float_constant(pass, "speed", 1.0)?,
            rough: effect_float_constant(pass, "rough", 0.2)?,
            noise_amount: effect_float_constant(pass, "noiseamount", 0.5)?,
            phase_offset: effect_float_constant(pass, "phase", 0.0)?,
            eye_color: effect_vec3_constant(pass, "color", [1.0, 1.0, 1.0])?,
            mask_combo,
            background_combo,
        })
    }
}

fn effect_combo_u32(
    pass: &SceneEffectPassGraphMaterialPass,
    name: &'static str,
) -> Result<u32, String> {
    let value = pass.combos.get(name).copied().unwrap_or(0);
    u32::try_from(value).map_err(|_| {
        format!(
            "scene iris effect pass {} for object {:?} combo {name} value {value} is outside u32",
            pass.pass_index, pass.object
        )
    })
}

fn effect_pass_texture_slot_mask(pass: &SceneEffectPassGraphMaterialPass) -> Result<u32, String> {
    let mut mask = 0u32;
    if let Some(source) = &pass.source {
        push_texture_slot(&mut mask, pass, source.slot)?;
    }
    for input in &pass.input_bindings {
        push_texture_slot(&mut mask, pass, input.slot)?;
    }
    for resource in &pass.texture_resources {
        push_texture_slot(&mut mask, pass, resource.slot)?;
    }
    Ok(mask)
}

fn push_texture_slot(
    mask: &mut u32,
    pass: &SceneEffectPassGraphMaterialPass,
    slot: u32,
) -> Result<(), String> {
    if slot >= u32::BITS {
        return Err(format!(
            "scene iris effect pass {} for object {:?} texture slot {slot} exceeds u32 mask",
            pass.pass_index, pass.object
        ));
    }
    *mask |= 1u32 << slot;
    Ok(())
}

fn effect_float_constant(
    pass: &SceneEffectPassGraphMaterialPass,
    name: &'static str,
    fallback: f32,
) -> Result<f32, String> {
    match pass.constants.get(name) {
        None => Ok(fallback),
        Some(SceneEffectConstantValue::Float(value)) => Ok(*value),
        Some(SceneEffectConstantValue::Integer(value)) => Ok(*value as f32),
        Some(SceneEffectConstantValue::String(value)) => {
            parse_float_string(value).ok_or_else(|| {
                format!(
                    "scene iris effect pass {} for object {:?} constant {name} expected float, got {:?}",
                    pass.pass_index,
                    pass.object,
                    SceneEffectConstantValue::String(value.clone())
                )
            })
        }
        Some(other) => Err(format!(
            "scene iris effect pass {} for object {:?} constant {name} expected float, got {other:?}",
            pass.pass_index, pass.object
        )),
    }
}

fn effect_vec2_constant(
    pass: &SceneEffectPassGraphMaterialPass,
    name: &'static str,
    fallback: [f32; 2],
) -> Result<[f32; 2], String> {
    match pass.constants.get(name) {
        None => Ok(fallback),
        Some(SceneEffectConstantValue::Vec2(value)) => Ok(*value),
        Some(SceneEffectConstantValue::Vec3(value)) => Ok([value[0], value[1]]),
        Some(SceneEffectConstantValue::Vec4(value)) => Ok([value[0], value[1]]),
        Some(SceneEffectConstantValue::Float(value)) => Ok([*value, *value]),
        Some(SceneEffectConstantValue::Integer(value)) => Ok([*value as f32, *value as f32]),
        Some(SceneEffectConstantValue::String(value)) => parse_vec2_string(value).ok_or_else(|| {
            format!(
                "scene iris effect pass {} for object {:?} constant {name} expected vec2, got {:?}",
                pass.pass_index,
                pass.object,
                SceneEffectConstantValue::String(value.clone())
            )
        }),
        Some(other) => Err(format!(
            "scene iris effect pass {} for object {:?} constant {name} expected vec2, got {other:?}",
            pass.pass_index, pass.object
        )),
    }
}

fn effect_vec3_constant(
    pass: &SceneEffectPassGraphMaterialPass,
    name: &'static str,
    fallback: [f32; 3],
) -> Result<[f32; 3], String> {
    match pass.constants.get(name) {
        None => Ok(fallback),
        Some(SceneEffectConstantValue::Vec3(value)) => Ok(*value),
        Some(SceneEffectConstantValue::Vec4(value)) => Ok([value[0], value[1], value[2]]),
        Some(SceneEffectConstantValue::Vec2(value)) => Ok([value[0], value[1], fallback[2]]),
        Some(SceneEffectConstantValue::String(value)) => parse_color_or_vec3_string(value).ok_or_else(|| {
            format!(
                "scene iris effect pass {} for object {:?} constant {name} expected vec3, got {:?}",
                pass.pass_index,
                pass.object,
                SceneEffectConstantValue::String(value.clone())
            )
        }),
        Some(other) => Err(format!(
            "scene iris effect pass {} for object {:?} constant {name} expected vec3, got {other:?}",
            pass.pass_index, pass.object
        )),
    }
}

fn parse_float_string(value: &str) -> Option<f32> {
    let lanes = parse_numeric_lanes(value)?;
    if lanes.len() == 1 {
        Some(lanes[0])
    } else {
        None
    }
}

fn parse_vec2_string(value: &str) -> Option<[f32; 2]> {
    let lanes = parse_numeric_lanes(value)?;
    if lanes.len() >= 2 {
        Some([lanes[0], lanes[1]])
    } else {
        None
    }
}

fn parse_color_or_vec3_string(value: &str) -> Option<[f32; 3]> {
    let trimmed = value.trim();
    if let Some(hex) = trimmed.strip_prefix('#')
        && (hex.len() == 6 || hex.len() == 8)
    {
        let r = u8::from_str_radix(&hex[0..2], 16).ok()? as f32 / 255.0;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()? as f32 / 255.0;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()? as f32 / 255.0;
        return Some([r, g, b]);
    }
    let lanes = parse_numeric_lanes(value)?;
    if lanes.len() >= 3 {
        Some([lanes[0], lanes[1], lanes[2]])
    } else {
        None
    }
}

fn parse_numeric_lanes(value: &str) -> Option<Vec<f32>> {
    let normalized = value.trim().trim_start_matches('[').trim_end_matches(']');
    let mut lanes = Vec::new();
    for lane in normalized.split(|ch: char| ch.is_ascii_whitespace() || ch == ',') {
        let lane = lane.trim();
        if lane.is_empty() {
            continue;
        }
        let value = lane.parse::<f32>().ok()?;
        if !value.is_finite() {
            return None;
        }
        lanes.push(value);
    }
    if lanes.is_empty() { None } else { Some(lanes) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene_engine::{
        SceneAlphaWriteMode, SceneCullMode, SceneDepthTest, SceneEffectPassBlend,
        SceneEffectPassGraphInputBinding, SceneEffectPassGraphInputSource,
        SceneEffectPassGraphOutput, SceneGraphTarget, SceneResourceId, we::WeEffectKind,
    };
    use std::collections::BTreeMap;

    #[test]
    fn iris_uniform_plan_lowers_we_defaults_and_overrides() {
        let mut pass = iris_pass();
        pass.combos.insert("BACKGROUND".to_owned(), 1);
        pass.combos.insert("MASK".to_owned(), 1);
        pass.constants.insert(
            "scale".to_owned(),
            SceneEffectConstantValue::String("2 3".to_owned()),
        );
        pass.constants
            .insert("speed".to_owned(), SceneEffectConstantValue::Float(1.5));
        pass.constants
            .insert("rough".to_owned(), SceneEffectConstantValue::Float(0.25));
        pass.constants.insert(
            "noiseamount".to_owned(),
            SceneEffectConstantValue::Float(0.75),
        );
        pass.constants
            .insert("phase".to_owned(), SceneEffectConstantValue::Float(-0.2));
        pass.constants.insert(
            "color".to_owned(),
            SceneEffectConstantValue::String("#1a334d".to_owned()),
        );
        let graph = SceneEffectPassGraphPlan {
            material_pass_count: 1,
            passes: vec![pass],
            ..SceneEffectPassGraphPlan::empty()
        };

        let plan = SceneEffectUniformFramePlan::from_effect_pass_graph(
            SceneFrameContext {
                time_ms: 1250,
                target_width: 3840,
                target_height: 2160,
            },
            &graph,
        )
        .expect("iris uniform frame plan");

        assert_eq!(plan.effect_pass_count, 1);
        assert_eq!(plan.iris_record_count, 1);
        let record = &plan.iris_records[0];
        assert_eq!(record.time_seconds, 1.25);
        assert_eq!(record.scale, [2.0, 3.0]);
        assert_eq!(record.speed, 1.5);
        assert_eq!(record.rough, 0.25);
        assert_eq!(record.noise_amount, 0.75);
        assert_eq!(record.phase_offset, -0.2);
        assert!((record.eye_color[0] - (26.0 / 255.0)).abs() < f32::EPSILON);
        assert!((record.eye_color[1] - (51.0 / 255.0)).abs() < f32::EPSILON);
        assert!((record.eye_color[2] - (77.0 / 255.0)).abs() < f32::EPSILON);
        assert_eq!(record.mask_combo, 1);
        assert_eq!(record.background_combo, 1);
        assert_eq!(record.texture_resolution_slots, vec![1]);
        assert_eq!(record.texture_slot_mask, 0b11);
    }

    #[test]
    fn iris_uniform_plan_does_not_require_mask_resolution_without_mask_combo() {
        let graph = SceneEffectPassGraphPlan {
            material_pass_count: 1,
            passes: vec![iris_pass()],
            ..SceneEffectPassGraphPlan::empty()
        };

        let plan = SceneEffectUniformFramePlan::from_effect_pass_graph(
            SceneFrameContext {
                time_ms: 0,
                target_width: 1920,
                target_height: 1080,
            },
            &graph,
        )
        .expect("iris uniform frame plan");

        let record = &plan.iris_records[0];
        assert_eq!(record.scale, [1.0, 1.0]);
        assert_eq!(record.speed, 1.0);
        assert_eq!(record.rough, 0.2);
        assert_eq!(record.noise_amount, 0.5);
        assert_eq!(record.phase_offset, 0.0);
        assert_eq!(record.eye_color, [1.0, 1.0, 1.0]);
        assert!(record.texture_resolution_slots.is_empty());
    }

    #[test]
    fn iris_uniform_plan_rejects_wrong_constant_type() {
        let mut pass = iris_pass();
        pass.constants.insert(
            "scale".to_owned(),
            SceneEffectConstantValue::String("wide".to_owned()),
        );
        let graph = SceneEffectPassGraphPlan {
            material_pass_count: 1,
            passes: vec![pass],
            ..SceneEffectPassGraphPlan::empty()
        };

        let err = SceneEffectUniformFramePlan::from_effect_pass_graph(
            SceneFrameContext {
                time_ms: 0,
                target_width: 1920,
                target_height: 1080,
            },
            &graph,
        )
        .expect_err("wrong constant type must fail");

        assert!(err.contains("constant scale expected vec2"));
    }

    fn iris_pass() -> SceneEffectPassGraphMaterialPass {
        SceneEffectPassGraphMaterialPass {
            graph_command_index: 0,
            graph_pass_index: 0,
            object: SceneObjectId(7),
            program_index: 0,
            pass_index: 0,
            effect_file: "effects/iris/effect.json".to_owned(),
            effect: WeEffectKind::Iris,
            shader: Some("effects/iris".to_owned()),
            source: Some(SceneEffectPassGraphInputBinding {
                slot: 0,
                image: crate::engine::scene_engine::SceneEffectImageRef::SourceTexture,
                source: SceneEffectPassGraphInputSource::ObjectSourceTexture(SceneResourceId(3)),
            }),
            input_bindings: vec![SceneEffectPassGraphInputBinding {
                slot: 1,
                image: crate::engine::scene_engine::SceneEffectImageRef::SourceTexture,
                source: SceneEffectPassGraphInputSource::ObjectSourceTexture(SceneResourceId(4)),
            }],
            output: SceneEffectPassGraphOutput::GraphTarget(SceneGraphTarget::EffectTarget(0)),
            blend: SceneEffectPassBlend::NormalReplace,
            depth_test: SceneDepthTest::Disabled,
            depth_write: false,
            cull_mode: SceneCullMode::None,
            alpha_write: SceneAlphaWriteMode::Default,
            texture_resources: Vec::new(),
            combos: BTreeMap::new(),
            constants: BTreeMap::new(),
        }
    }
}
