//! Material texture-slot and image-effect pass lowering for legacy `.gscn` layers.
//!
//! References:
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/effects/iris.md`
//! - `references/godot/servers/rendering/renderer_scene_render.h`

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;

use crate::core::scene::binary::{
    SCENE_BINARY_EFFECT_PARAMETER_RECORD_SIZE, SCENE_BINARY_EFFECT_UV_MAPPING_TEXTURE_RESOLUTION,
    SCENE_BINARY_EFFECT_UV_TRANSFORM_RECORD_SIZE, SCENE_BINARY_NONE_ID,
    SCENE_BINARY_PARAMETER_ROLE_EFFECT_FBO, SCENE_BINARY_PARAMETER_ROLE_PASS_BIND,
    SCENE_BINARY_PARAMETER_ROLE_PASS_COMBO, SCENE_BINARY_PARAMETER_ROLE_PASS_CONSTANT,
    SCENE_BINARY_PARAMETER_VALUE_BOOL, SCENE_BINARY_PARAMETER_VALUE_FLOAT,
    SCENE_BINARY_PARAMETER_VALUE_INTEGER, SCENE_BINARY_PARAMETER_VALUE_STRING,
    SCENE_BINARY_PARAMETER_VALUE_VEC2, SCENE_BINARY_PARAMETER_VALUE_VEC3,
    SCENE_BINARY_PARAMETER_VALUE_VEC4, SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE, SceneBinaryChunkKind,
    SceneBinaryEffectParameterRecord, SceneBinaryEffectPassRecord,
    SceneBinaryEffectUvTransformRecord, SceneBinaryMaterialPassRecord,
    SceneBinaryTextureSlotRecord, decode_effect_parameter_record, decode_effect_pass_record,
    decode_effect_uv_transform_record, decode_texture_slot_record,
};
use crate::core::scene::{
    SceneEffectFbo, SceneEffectUvExtent, SceneEffectUvMapping, SceneEffectUvTransform,
};
use crate::renderer::{
    RendererPlanError, SceneRenderAlphaTextureMode, SceneRenderImageEffectPass,
    SceneRenderTextureSlot,
};

use super::super::facts::{BinarySceneNames, BinarySceneResource, binary_name};
use super::super::reader::BinarySceneReader;
use super::super::schema::{
    BINARY_EFFECT_UV_HAS_INPUT_EXTENT, BINARY_EFFECT_UV_HAS_MASK_BACKING_EXTENT,
    BINARY_EFFECT_UV_HAS_MASK_EXTENT, BINARY_TEXTURE_ROLE_BASE_COLOR,
};

pub(in crate::renderer::scene_binary) fn binary_scene_material_texture_slots_cached(
    reader: &mut BinarySceneReader,
    material_index: u32,
    material: SceneBinaryMaterialPassRecord,
    resources: &[BinarySceneResource],
) -> Result<Vec<SceneRenderTextureSlot>, RendererPlanError> {
    if let Some(slots) = reader.material_texture_slots_cache.get(&material_index) {
        return Ok((**slots).clone());
    }
    let slots = Arc::new(binary_scene_material_texture_slots(
        reader, material, resources,
    )?);
    reader
        .material_texture_slots_cache
        .insert(material_index, Arc::clone(&slots));
    Ok((*slots).clone())
}

pub(super) fn binary_scene_image_effect_passes_cached(
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

pub(super) fn binary_scene_alpha_texture_slot(
    material: SceneBinaryMaterialPassRecord,
) -> Option<u32> {
    (material.alpha_texture_slot != SCENE_BINARY_NONE_ID).then_some(material.alpha_texture_slot)
}

pub(super) fn binary_scene_alpha_texture_mode(
    material: SceneBinaryMaterialPassRecord,
) -> SceneRenderAlphaTextureMode {
    match material.alpha_texture_mode {
        2 => SceneRenderAlphaTextureMode::Inverse,
        3 => SceneRenderAlphaTextureMode::Iris,
        4 => SceneRenderAlphaTextureMode::Coverage,
        _ => SceneRenderAlphaTextureMode::Multiply,
    }
}

fn binary_scene_material_texture_slots(
    reader: &mut BinarySceneReader,
    material: SceneBinaryMaterialPassRecord,
    resources: &[BinarySceneResource],
) -> Result<Vec<SceneRenderTextureSlot>, RendererPlanError> {
    let slots = reader.record_range(
        SceneBinaryChunkKind::TextureSlots,
        SCENE_BINARY_TEXTURE_SLOT_RECORD_SIZE,
        material.first_texture_slot,
        material.texture_slot_count,
        decode_texture_slot_record,
    )?;
    binary_scene_texture_slots(slots, resources, |slot| {
        slot.role_flags & BINARY_TEXTURE_ROLE_BASE_COLOR != 0
    })
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

fn binary_scene_texture_slots(
    slots: Vec<SceneBinaryTextureSlotRecord>,
    resources: &[BinarySceneResource],
    keep: impl Fn(&SceneBinaryTextureSlotRecord) -> bool,
) -> Result<Vec<SceneRenderTextureSlot>, RendererPlanError> {
    let mut output = Vec::with_capacity(slots.len());
    for slot in slots {
        if !keep(&slot) {
            continue;
        }
        let Some(resource) = resources.get(slot.resource_index as usize) else {
            continue;
        };
        let Some(source) = resource.source.clone() else {
            continue;
        };
        output.push(SceneRenderTextureSlot {
            slot: slot.slot,
            source,
            width: resource.width.or((slot.width > 0).then_some(slot.width)),
            height: resource.height.or((slot.height > 0).then_some(slot.height)),
        });
    }
    Ok(output)
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

fn binary_scene_effect_runtime(kind: u16, effect_file: &str) -> Option<String> {
    let normalized = effect_file.replace('\\', "/").to_ascii_lowercase();
    let runtime = match kind {
        1 => "native-opacity-mask",
        2 => "native-iris-mask",
        3..=5 => return None,
        7 if normalized.contains("foliagesway")
            || normalized.contains("foliage_sway")
            || normalized.contains("auto_sway")
            || normalized.contains("autosway") =>
        {
            return None;
        }
        7..=9 => "native-effect-motion",
        6 => "native-water-caustics",
        _ if normalized.ends_with("effects/opacity/effect.json") => "native-opacity-mask",
        _ if normalized.ends_with("effects/iris/effect.json") => "native-iris-mask",
        _ if normalized.contains("waterripple")
            || normalized.contains("waterwaves")
            || normalized.contains("waterflow") =>
        {
            return None;
        }
        _ if normalized.contains("foliagesway")
            || normalized.contains("foliage_sway")
            || normalized.contains("auto_sway")
            || normalized.contains("autosway") =>
        {
            return None;
        }
        _ if normalized.contains("sway")
            || normalized.contains("shake")
            || normalized.contains("flutter")
            || normalized.contains("drift") =>
        {
            "native-effect-motion"
        }
        _ => return None,
    };
    Some(runtime.to_owned())
}

fn binary_scene_material_flag(code: u16) -> Option<String> {
    match code {
        1 => Some("enabled".to_owned()),
        2 => Some("disabled".to_owned()),
        _ => None,
    }
}

fn binary_scene_cull_mode(code: u16) -> Option<String> {
    match code {
        1 => Some("disabled".to_owned()),
        2 => Some("back".to_owned()),
        3 => Some("front".to_owned()),
        4 => Some("frontandback".to_owned()),
        5 => Some("unknown".to_owned()),
        _ => None,
    }
}
