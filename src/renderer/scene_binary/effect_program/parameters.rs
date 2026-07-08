//! WE effect pass parameter lowering.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/fluidsimulation.md`
//! - `reverse-engineered/effects/iris.md`

use std::collections::BTreeMap;

use crate::core::scene::binary::{
    SCENE_BINARY_PARAMETER_ROLE_EFFECT_FBO, SCENE_BINARY_PARAMETER_ROLE_PASS_BIND,
    SCENE_BINARY_PARAMETER_ROLE_PASS_COMBO, SCENE_BINARY_PARAMETER_ROLE_PASS_CONSTANT,
    SCENE_BINARY_PARAMETER_VALUE_BOOL, SCENE_BINARY_PARAMETER_VALUE_FLOAT,
    SCENE_BINARY_PARAMETER_VALUE_INTEGER, SCENE_BINARY_PARAMETER_VALUE_STRING,
    SCENE_BINARY_PARAMETER_VALUE_VEC2, SCENE_BINARY_PARAMETER_VALUE_VEC3,
    SCENE_BINARY_PARAMETER_VALUE_VEC4, SceneBinaryChunkKind, SceneBinaryEffectParameterRecord,
    SceneBinaryEffectPassRecord, decode_effect_parameter_record,
};
use crate::engine::scene_engine::{
    SceneEffectConstantValue, SceneEffectFboFormat, SceneEffectImageRef,
};
use crate::renderer::RendererPlanError;

use super::super::facts::{BinarySceneNames, binary_name};
use super::super::reader::BinarySceneReader;

pub(super) struct GscnEffectPassParameters {
    pub(super) fbos: Vec<GscnEffectFboFact>,
    pub(super) binds: BTreeMap<u32, SceneEffectImageRef>,
    pub(super) combos: BTreeMap<String, i64>,
    pub(super) constants: BTreeMap<String, SceneEffectConstantValue>,
}

#[derive(Clone)]
pub(super) struct GscnEffectFboFact {
    pub(super) name: String,
    pub(super) format: Option<SceneEffectFboFormat>,
    pub(super) scale: f32,
    pub(super) unique: bool,
}

pub(super) fn gscn_effect_pass_parameters(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    pass: SceneBinaryEffectPassRecord,
) -> Result<GscnEffectPassParameters, RendererPlanError> {
    let parameters = reader.record_range(
        SceneBinaryChunkKind::EffectParameter,
        crate::core::scene::binary::SCENE_BINARY_EFFECT_PARAMETER_RECORD_SIZE,
        pass.first_parameter,
        pass.parameter_count,
        decode_effect_parameter_record,
    )?;
    let mut fbos = Vec::new();
    let mut binds = BTreeMap::new();
    let mut combos = BTreeMap::new();
    let mut constants = BTreeMap::new();
    for parameter in parameters {
        if parameter.role_flags & SCENE_BINARY_PARAMETER_ROLE_EFFECT_FBO != 0 {
            if let Some(name) = binary_name(names, parameter.parameter_name) {
                fbos.push(GscnEffectFboFact {
                    name: name.to_owned(),
                    format: binary_name(names, parameter.value_name)
                        .map(SceneEffectFboFormat::from_we_name),
                    scale: if parameter.value0.is_finite() && parameter.value0 > 0.0 {
                        parameter.value0
                    } else {
                        1.0
                    },
                    unique: parameter.integer_value != 0,
                });
            }
            continue;
        }
        if parameter.role_flags & SCENE_BINARY_PARAMETER_ROLE_PASS_BIND != 0 {
            let slot = u32::try_from(parameter.integer_value)
                .ok()
                .or_else(|| {
                    binary_name(names, parameter.parameter_name).and_then(|name| name.parse().ok())
                })
                .unwrap_or(0);
            if let Some(name) = binary_name(names, parameter.value_name) {
                binds.insert(slot, SceneEffectImageRef::from_we_name(name));
            }
            continue;
        }
        let Some(name) = binary_name(names, parameter.parameter_name) else {
            continue;
        };
        if parameter.role_flags & SCENE_BINARY_PARAMETER_ROLE_PASS_COMBO != 0 {
            combos.insert(name.to_owned(), parameter.integer_value);
            continue;
        }
        if parameter.role_flags & SCENE_BINARY_PARAMETER_ROLE_PASS_CONSTANT != 0
            && let Some(value) = gscn_effect_constant_value(names, parameter)?
        {
            constants.insert(name.to_owned(), value);
        }
    }
    Ok(GscnEffectPassParameters {
        fbos,
        binds,
        combos,
        constants,
    })
}

fn gscn_effect_constant_value(
    names: &BinarySceneNames,
    parameter: SceneBinaryEffectParameterRecord,
) -> Result<Option<SceneEffectConstantValue>, RendererPlanError> {
    let finite = |values: &[f32]| values.iter().all(|value| value.is_finite());
    let value = match parameter.value_kind {
        SCENE_BINARY_PARAMETER_VALUE_BOOL => {
            SceneEffectConstantValue::Bool(parameter.integer_value != 0)
        }
        SCENE_BINARY_PARAMETER_VALUE_FLOAT if finite(&[parameter.value0]) => {
            SceneEffectConstantValue::Float(parameter.value0)
        }
        SCENE_BINARY_PARAMETER_VALUE_INTEGER => {
            SceneEffectConstantValue::Integer(parameter.integer_value)
        }
        SCENE_BINARY_PARAMETER_VALUE_STRING => {
            let Some(value) = binary_name(names, parameter.value_name) else {
                return Ok(None);
            };
            SceneEffectConstantValue::String(value.to_owned())
        }
        SCENE_BINARY_PARAMETER_VALUE_VEC2 if finite(&[parameter.value0, parameter.value1]) => {
            SceneEffectConstantValue::Vec2([parameter.value0, parameter.value1])
        }
        SCENE_BINARY_PARAMETER_VALUE_VEC3
            if finite(&[parameter.value0, parameter.value1, parameter.value2]) =>
        {
            SceneEffectConstantValue::Vec3([parameter.value0, parameter.value1, parameter.value2])
        }
        SCENE_BINARY_PARAMETER_VALUE_VEC4
            if finite(&[
                parameter.value0,
                parameter.value1,
                parameter.value2,
                parameter.value3,
            ]) =>
        {
            SceneEffectConstantValue::Vec4([
                parameter.value0,
                parameter.value1,
                parameter.value2,
                parameter.value3,
            ])
        }
        SCENE_BINARY_PARAMETER_VALUE_FLOAT
        | SCENE_BINARY_PARAMETER_VALUE_VEC2
        | SCENE_BINARY_PARAMETER_VALUE_VEC3
        | SCENE_BINARY_PARAMETER_VALUE_VEC4 => {
            return Err(RendererPlanError::PackageLoad(format!(
                "WE effect parameter {:?} contains non-finite float lanes",
                binary_name(names, parameter.parameter_name).unwrap_or("<unnamed>")
            )));
        }
        _ => return Ok(None),
    };
    Ok(Some(value))
}
