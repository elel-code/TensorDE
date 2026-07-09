//! Scene material/effect constant packing for the Vulkanalia scene runtime.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/docs/material-format.md`
//! - `reverse-engineered/docs/effect-format.md`
//! - `reverse-engineered/docs/exe/global-uniforms.md`

use std::mem::size_of;

use serde_json::Value;

use crate::engine::scene::{
    INVALID_MATERIAL_ID, SceneMaterialConstantRecord, SceneMaterialHandle,
    SceneMaterialPassRecord, SceneRenderingDeviceMeshDraw, SceneStorage,
};

const SCENE_MATERIAL_UNIFORM_FLOATS: usize = 12;

pub(super) fn pack_scene_material_uniforms(
    storage: &SceneStorage,
    draws: &[SceneRenderingDeviceMeshDraw],
) -> Vec<u8> {
    let mut payload =
        Vec::with_capacity(draws.len() * SCENE_MATERIAL_UNIFORM_FLOATS * size_of::<f32>());
    for draw in draws {
        for value in material_uniform_values(storage, draw.material) {
            payload.extend_from_slice(&value.to_le_bytes());
        }
    }
    payload
}

fn material_uniform_values(
    storage: &SceneStorage,
    material: SceneMaterialHandle,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = default_material_uniform_values();
    let Some(pass) = first_material_pass(storage, material) else {
        return values;
    };

    let mut scalar_lane = 4;
    for constant in material_pass_constants(storage, pass) {
        let name = storage.string(constant.name).unwrap_or_default();
        let value_json = storage.string(constant.value_json).unwrap_or_default();
        let constant_values = parse_constant_values(value_json);
        if constant_values.is_empty() {
            continue;
        }
        if is_color_constant(name) {
            for (lane, value) in constant_values.iter().take(4).enumerate() {
                values[lane] = *value;
            }
            continue;
        }
        for value in constant_values {
            if scalar_lane >= values.len() {
                break;
            }
            values[scalar_lane] = value;
            scalar_lane += 1;
        }
    }
    values
}

fn default_material_uniform_values() -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    [1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]
}

fn first_material_pass(
    storage: &SceneStorage,
    material: SceneMaterialHandle,
) -> Option<&SceneMaterialPassRecord> {
    if material.0 == INVALID_MATERIAL_ID {
        return None;
    }
    let material = storage.document().materials.get(material.0 as usize)?;
    storage
        .document()
        .material_passes
        .get(material.pass_start as usize)
}

fn material_pass_constants<'a>(
    storage: &'a SceneStorage,
    pass: &SceneMaterialPassRecord,
) -> &'a [SceneMaterialConstantRecord] {
    let start = pass.constant_start as usize;
    let end = start.saturating_add(pass.constant_count as usize);
    storage
        .document()
        .material_constants
        .get(start..end)
        .unwrap_or(&[])
}

fn parse_constant_values(value_json: &str) -> Vec<f32> {
    let Ok(value) = serde_json::from_str::<Value>(value_json) else {
        return Vec::new();
    };
    let mut values = Vec::new();
    collect_constant_values(&value, &mut values);
    values
}

fn collect_constant_values(value: &Value, out: &mut Vec<f32>) {
    match value {
        Value::Number(number) => {
            if let Some(value) = number.as_f64().filter(|value| value.is_finite()) {
                out.push(value as f32);
            }
        }
        Value::Bool(value) => out.push(if *value { 1.0 } else { 0.0 }),
        Value::String(value) => {
            if let Ok(value) = value.parse::<f32>() {
                out.push(value);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_constant_values(value, out);
            }
        }
        Value::Object(object) => {
            if let Some(value) = object.get("value") {
                collect_constant_values(value, out);
                return;
            }
            for key in ["x", "y", "z", "w", "r", "g", "b", "a"] {
                if let Some(value) = object.get(key) {
                    collect_constant_values(value, out);
                }
            }
        }
        Value::Null => {}
    }
}

fn is_color_constant(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name == "color"
        || name == "color4"
        || name == "tint"
        || name == "g_color"
        || name == "g_color4"
        || name.contains("color")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{
        SceneBinaryDocument, SceneCullMode, SceneDepthTest, SceneMaterialConstantRecord,
        SceneMaterialHandle, SceneMaterialPassRecord, SceneMaterialRecord, ScenePipelineBlend,
        SceneRenderingDeviceDrawPrimitive, SceneResourceId, SceneStringId,
    };

    #[test]
    fn material_uniform_uses_default_when_draw_has_no_material() {
        let storage = SceneStorage::from_document(SceneBinaryDocument::default()).expect("storage");
        let draw = draw_with_material(SceneMaterialHandle(INVALID_MATERIAL_ID));

        let payload = pack_scene_material_uniforms(&storage, &[draw]);

        assert_eq!(payload.len(), SCENE_MATERIAL_UNIFORM_FLOATS * size_of::<f32>());
        assert_eq!(f32_from_payload(&payload, 0), 1.0);
        assert_eq!(f32_from_payload(&payload, 12), 1.0);
        assert_eq!(f32_from_payload(&payload, 16), 0.0);
    }

    #[test]
    fn material_uniform_packs_color_constant_into_first_vec4() {
        let storage = SceneStorage::from_document(SceneBinaryDocument {
            strings: vec![
                "tint".to_owned(),
                "[0.25,0.5,0.75,0.9]".to_owned(),
                "speed".to_owned(),
                "2.5".to_owned(),
            ],
            materials: vec![SceneMaterialRecord {
                id: SceneMaterialHandle(0),
                resource: SceneResourceId::NONE,
                pass_start: 0,
                pass_count: 1,
            }],
            material_passes: vec![SceneMaterialPassRecord {
                material: SceneMaterialHandle(0),
                shader_key: SceneStringId::NONE,
                target: SceneStringId::NONE,
                texture_start: 0,
                texture_count: 0,
                constant_start: 0,
                constant_count: 2,
                pipeline_blend: ScenePipelineBlend::Normal,
                depth_test: SceneDepthTest::Disabled,
                depth_write: false,
                cull_mode: SceneCullMode::None,
                alpha_writing: SceneStringId::NONE,
                clear_target: false,
            }],
            material_constants: vec![
                SceneMaterialConstantRecord {
                    name: SceneStringId(0),
                    value_json: SceneStringId(1),
                },
                SceneMaterialConstantRecord {
                    name: SceneStringId(2),
                    value_json: SceneStringId(3),
                },
            ],
            ..SceneBinaryDocument::default()
        })
        .expect("storage");
        let draw = draw_with_material(SceneMaterialHandle(0));

        let payload = pack_scene_material_uniforms(&storage, &[draw]);

        assert_eq!(f32_from_payload(&payload, 0), 0.25);
        assert_eq!(f32_from_payload(&payload, 4), 0.5);
        assert_eq!(f32_from_payload(&payload, 8), 0.75);
        assert_eq!(f32_from_payload(&payload, 12), 0.9);
        assert_eq!(f32_from_payload(&payload, 16), 2.5);
    }

    fn draw_with_material(material: SceneMaterialHandle) -> SceneRenderingDeviceMeshDraw {
        SceneRenderingDeviceMeshDraw {
            primitive: SceneRenderingDeviceDrawPrimitive::ObjectMesh,
            mesh_index: 0,
            resolved_object_index: 0,
            clip_transform: [[0.0; 4]; 4],
            skinning_palette_start: crate::engine::scene::INVALID_OBJECT_ID,
            skinning_palette_count: 0,
            object: crate::engine::scene::SceneObjectHandle(0),
            material,
            vertex_start: 0,
            vertex_count: 4,
            index_start: 0,
            index_count: 6,
        }
    }

    fn f32_from_payload(payload: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap())
    }
}
