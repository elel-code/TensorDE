//! Image-effect pass lowering for binary scene render layers.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/effects/iris.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;

use crate::core::scene::binary::{
    SCENE_BINARY_EFFECT_PARAMETER_RECORD_SIZE, SCENE_BINARY_EFFECT_UV_MAPPING_TEXTURE_RESOLUTION,
    SCENE_BINARY_EFFECT_UV_TRANSFORM_RECORD_SIZE, SCENE_BINARY_PARAMETER_ROLE_EFFECT_FBO,
    SCENE_BINARY_PARAMETER_ROLE_PASS_BIND, SCENE_BINARY_PARAMETER_ROLE_PASS_COMBO,
    SCENE_BINARY_PARAMETER_ROLE_PASS_CONSTANT, SCENE_BINARY_PARAMETER_VALUE_BOOL,
    SCENE_BINARY_PARAMETER_VALUE_FLOAT, SCENE_BINARY_PARAMETER_VALUE_INTEGER,
    SCENE_BINARY_PARAMETER_VALUE_STRING, SCENE_BINARY_PARAMETER_VALUE_VEC2,
    SCENE_BINARY_PARAMETER_VALUE_VEC3, SCENE_BINARY_PARAMETER_VALUE_VEC4,
    SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE, SceneBinaryChunkKind, SceneBinaryEffectParameterRecord,
    SceneBinaryEffectPassRecord, SceneBinaryEffectUvTransformRecord, SceneBinaryMaterialPassRecord,
    decode_effect_parameter_record, decode_effect_pass_record, decode_effect_uv_transform_record,
    decode_texture_slot_record,
};
use crate::core::scene::{
    SceneEffectFbo, SceneEffectUvExtent, SceneEffectUvMapping, SceneEffectUvTransform,
};
use crate::renderer::{RendererPlanError, SceneRenderImageEffectPass};

use super::super::super::facts::{BinarySceneNames, BinarySceneResource, binary_name};
use super::super::super::reader::BinarySceneReader;
use super::super::super::schema::{
    BINARY_EFFECT_UV_HAS_INPUT_EXTENT, BINARY_EFFECT_UV_HAS_MASK_BACKING_EXTENT,
    BINARY_EFFECT_UV_HAS_MASK_EXTENT,
};
use super::effect_runtime::binary_scene_effect_runtime;
use super::render_state::{binary_scene_cull_mode, binary_scene_material_flag};
use super::texture_slots::binary_scene_texture_slots;

pub(in crate::renderer::scene_binary::render_layers) fn binary_scene_image_effect_passes_cached(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    material_index: u32,
    material: SceneBinaryMaterialPassRecord,
    resources: &[BinarySceneResource],
) -> Result<Vec<SceneRenderImageEffectPass>, RendererPlanError> {
    if let Some(passes) = reader.material_effect_passes_cache.get(&material_index) {
        return Ok((**passes).clone());
    }
    let passes = Arc::new(binary_scene_image_effect_passes(
        reader, names, material, resources,
    )?);
    reader
        .material_effect_passes_cache
        .insert(material_index, Arc::clone(&passes));
    Ok((*passes).clone())
}

fn binary_scene_image_effect_passes(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    material: SceneBinaryMaterialPassRecord,
    resources: &[BinarySceneResource],
) -> Result<Vec<SceneRenderImageEffectPass>, RendererPlanError> {
    let passes = reader.record_range(
        SceneBinaryChunkKind::EffectPass,
        reader.layout_record_size(SceneBinaryChunkKind::EffectPass)?,
        material.first_effect_pass,
        material.effect_pass_count,
        decode_effect_pass_record,
    )?;
    let mut output = Vec::with_capacity(passes.len());
    for pass in passes {
        output.push(binary_scene_image_effect_pass(
            reader, names, resources, pass,
        )?);
    }
    Ok(output)
}

fn binary_scene_image_effect_pass(
    reader: &mut BinarySceneReader,
    names: &BinarySceneNames,
    resources: &[BinarySceneResource],
    pass: SceneBinaryEffectPassRecord,
) -> Result<SceneRenderImageEffectPass, RendererPlanError> {
    let texture_slots = reader.record_range(
        SceneBinaryChunkKind::TextureSlots,
        SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE,
        pass.first_texture_slot,
        pass.texture_slot_count,
        decode_texture_slot_record,
    )?;
    let transforms = reader.record_range(
        SceneBinaryChunkKind::EffectUvTransform,
        SCENE_BINARY_EFFECT_UV_TRANSFORM_RECORD_SIZE,
        pass.first_effect_uv_transform,
        pass.effect_uv_transform_count,
        decode_effect_uv_transform_record,
    )?;
    let parameters = reader.record_range(
        SceneBinaryChunkKind::EffectParameter,
        SCENE_BINARY_EFFECT_PARAMETER_RECORD_SIZE,
        pass.first_parameter,
        pass.parameter_count,
        decode_effect_parameter_record,
    )?;
    let effect_file = binary_name(names, pass.effect_name)
        .unwrap_or("")
        .to_owned();
    let shader = binary_name(names, pass.shader_name).map(str::to_owned);
    let blending = binary_name(names, pass.blending_name).map(str::to_owned);
    let command = binary_name(names, pass.command_name).map(str::to_owned);
    let source = binary_name(names, pass.source_name).map(str::to_owned);
    let target = binary_name(names, pass.target_name).map(str::to_owned);
    let (binds, fbos, combos, constant_shader_values) =
        binary_scene_image_effect_parameters(names, parameters);
    Ok(SceneRenderImageEffectPass {
        effect_file: effect_file.clone(),
        runtime: binary_scene_effect_runtime(pass.kind, &effect_file),
        pass_index: pass.pass_index as usize,
        command,
        source,
        target,
        binds,
        fbos,
        shader,
        blending,
        depthtest: binary_scene_material_flag(pass.depth_test),
        depthwrite: binary_scene_material_flag(pass.depth_write),
        cullmode: binary_scene_cull_mode(pass.cull_mode),
        alphawriting: binary_scene_material_flag(pass.alpha_write),
        texture_slots: binary_scene_texture_slots(texture_slots, resources, |_| true)?,
        effect_uv_transform: transforms
            .into_iter()
            .next()
            .map(binary_scene_effect_uv_transform),
        combos,
        constant_shader_values,
    })
}

fn binary_scene_image_effect_parameters(
    names: &BinarySceneNames,
    parameters: Vec<SceneBinaryEffectParameterRecord>,
) -> (
    BTreeMap<u32, String>,
    Vec<SceneEffectFbo>,
    BTreeMap<String, i64>,
    BTreeMap<String, Value>,
) {
    let mut binds = BTreeMap::new();
    let mut fbos = Vec::new();
    let mut combos = BTreeMap::new();
    let mut constants = BTreeMap::new();
    for parameter in parameters {
        if parameter.role_flags & SCENE_BINARY_PARAMETER_ROLE_EFFECT_FBO != 0 {
            if let Some(name) = binary_name(names, parameter.parameter_name) {
                fbos.push(SceneEffectFbo {
                    name: name.to_owned(),
                    format: binary_name(names, parameter.value_name).map(str::to_owned),
                    scale: if parameter.value0.is_finite() && parameter.value0 > 0.0 {
                        parameter.value0 as f64
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
                binds.insert(slot, name.to_owned());
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
            && let Some(value) = binary_scene_effect_parameter_value(names, parameter)
        {
            constants.insert(name.to_owned(), value);
        }
    }
    (binds, fbos, combos, constants)
}

fn binary_scene_effect_parameter_value(
    names: &BinarySceneNames,
    parameter: SceneBinaryEffectParameterRecord,
) -> Option<Value> {
    match parameter.value_kind {
        SCENE_BINARY_PARAMETER_VALUE_BOOL => Some(Value::Bool(parameter.integer_value != 0)),
        SCENE_BINARY_PARAMETER_VALUE_FLOAT => {
            serde_json::Number::from_f64(parameter.value0 as f64).map(Value::Number)
        }
        SCENE_BINARY_PARAMETER_VALUE_INTEGER => Some(Value::Number(serde_json::Number::from(
            parameter.integer_value,
        ))),
        SCENE_BINARY_PARAMETER_VALUE_STRING => binary_name(names, parameter.value_name)
            .map(str::to_owned)
            .map(Value::String),
        SCENE_BINARY_PARAMETER_VALUE_VEC2 => Some(Value::Array(vec![
            Value::from(parameter.value0 as f64),
            Value::from(parameter.value1 as f64),
        ])),
        SCENE_BINARY_PARAMETER_VALUE_VEC3 => Some(Value::Array(vec![
            Value::from(parameter.value0 as f64),
            Value::from(parameter.value1 as f64),
            Value::from(parameter.value2 as f64),
        ])),
        SCENE_BINARY_PARAMETER_VALUE_VEC4 => Some(Value::Array(vec![
            Value::from(parameter.value0 as f64),
            Value::from(parameter.value1 as f64),
            Value::from(parameter.value2 as f64),
            Value::from(parameter.value3 as f64),
        ])),
        _ => None,
    }
}

fn binary_scene_effect_uv_transform(
    record: SceneBinaryEffectUvTransformRecord,
) -> SceneEffectUvTransform {
    SceneEffectUvTransform {
        mapping: match record.mapping {
            SCENE_BINARY_EFFECT_UV_MAPPING_TEXTURE_RESOLUTION => {
                SceneEffectUvMapping::TextureResolution
            }
            _ => SceneEffectUvMapping::TextureResolution,
        },
        source_slot: record.source_slot,
        mask_slot: record.mask_slot,
        scale: [f64::from(record.scale_u), f64::from(record.scale_v)],
        offset: [f64::from(record.offset_u), f64::from(record.offset_v)],
        input_extent: (record.flags & BINARY_EFFECT_UV_HAS_INPUT_EXTENT != 0)
            .then(|| binary_scene_effect_uv_extent(record.input_width, record.input_height))
            .flatten(),
        mask_extent: (record.flags & BINARY_EFFECT_UV_HAS_MASK_EXTENT != 0)
            .then(|| binary_scene_effect_uv_extent(record.mask_width, record.mask_height))
            .flatten(),
        mask_backing_extent: (record.flags & BINARY_EFFECT_UV_HAS_MASK_BACKING_EXTENT != 0)
            .then(|| binary_scene_effect_uv_extent(record.backing_width, record.backing_height))
            .flatten(),
    }
}

fn binary_scene_effect_uv_extent(width: u32, height: u32) -> Option<SceneEffectUvExtent> {
    (width > 0 && height > 0).then_some(SceneEffectUvExtent { width, height })
}
