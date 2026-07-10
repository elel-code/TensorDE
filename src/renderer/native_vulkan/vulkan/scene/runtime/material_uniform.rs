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
use crate::renderer::native_vulkan::scene::{
    BuiltinSceneParameterLayout, native_vulkan_scene_shader_for_key,
};

pub(super) const SCENE_MATERIAL_UNIFORM_BYTES: u64 = 64;
const SCENE_MATERIAL_UNIFORM_FLOATS: usize =
    SCENE_MATERIAL_UNIFORM_BYTES as usize / size_of::<f32>();

pub(super) fn pack_scene_material_uniforms(
    storage: &SceneStorage,
    draws: &[SceneRenderingDeviceMeshDraw],
    scene_time_seconds: f32,
) -> Vec<u8> {
    let mut payload =
        Vec::with_capacity(draws.len() * SCENE_MATERIAL_UNIFORM_FLOATS * size_of::<f32>());
    for draw in draws {
        for value in material_uniform_values(storage, draw.material, scene_time_seconds) {
            payload.extend_from_slice(&value.to_le_bytes());
        }
    }
    payload
}

fn material_uniform_values(
    storage: &SceneStorage,
    material: SceneMaterialHandle,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let Some(pass) = first_material_pass(storage, material) else {
        return [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    };
    let shader_key = storage.string(pass.shader_key).unwrap_or_default();
    let layout = native_vulkan_scene_shader_for_key(shader_key)
        .map(|shader| shader.parameter_layout)
        .unwrap_or(BuiltinSceneParameterLayout::None);
    let parameters = MaterialParameters { storage, pass };
    match layout {
        BuiltinSceneParameterLayout::None => [0.0; SCENE_MATERIAL_UNIFORM_FLOATS],
        BuiltinSceneParameterLayout::StandardMaterial => standard_material_values(&parameters),
        BuiltinSceneParameterLayout::Iris => iris_fragment_values(&parameters, shader_key),
        BuiltinSceneParameterLayout::Opacity => opacity_values(&parameters),
        BuiltinSceneParameterLayout::WaterWaves => {
            waterwaves_values(&parameters, shader_key, scene_time_seconds)
        }
        BuiltinSceneParameterLayout::WaterRipple => {
            waterripple_values(&parameters, shader_key, scene_time_seconds)
        }
    }
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

pub(super) fn material_pass_constants<'a>(
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

pub(super) fn material_parameter_values(
    storage: &SceneStorage,
    material: SceneMaterialHandle,
    names: &[&str],
) -> Vec<f32> {
    let Some(pass) = first_material_pass(storage, material) else {
        return Vec::new();
    };
    MaterialParameters { storage, pass }.values(names)
}

pub(super) fn material_parameter_layout(
    storage: &SceneStorage,
    material: SceneMaterialHandle,
) -> BuiltinSceneParameterLayout {
    first_material_pass(storage, material)
        .and_then(|pass| storage.string(pass.shader_key))
        .and_then(native_vulkan_scene_shader_for_key)
        .map(|shader| shader.parameter_layout)
        .unwrap_or(BuiltinSceneParameterLayout::None)
}

struct MaterialParameters<'a> {
    storage: &'a SceneStorage,
    pass: &'a SceneMaterialPassRecord,
}

impl MaterialParameters<'_> {
    fn values(&self, names: &[&str]) -> Vec<f32> {
        for name in names {
            if let Some(values) = material_pass_constants(self.storage, self.pass)
                .iter()
                .find_map(|constant| {
                    let constant_name = self.storage.string(constant.name)?;
                    constant_name.eq_ignore_ascii_case(name).then(|| {
                        parse_constant_values(
                            self.storage.string(constant.value_json).unwrap_or_default(),
                        )
                    })
                })
                .filter(|values| !values.is_empty())
            {
                return values;
            }
        }
        Vec::new()
    }

    fn scalar(&self, names: &[&str], default: f32) -> f32 {
        self.values(names).first().copied().unwrap_or(default)
    }
}

fn standard_material_values(
    parameters: &MaterialParameters<'_>,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[..4].copy_from_slice(&[1.0, 1.0, 1.0, 1.0]);
    set_vector(
        &mut values,
        0,
        &parameters.values(&["color4", "g_color4", "color", "tint"]),
        4,
    );
    values[4] = parameters.scalar(&["roughness"], 0.0);
    values[5] = parameters.scalar(&["metallic"], 0.0);
    set_vector(
        &mut values,
        8,
        &parameters.values(&["speculartint", "specularcolor"]),
        4,
    );
    values
}

fn iris_fragment_values(
    parameters: &MaterialParameters<'_>,
    shader_key: &str,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[..3].copy_from_slice(&[1.0, 1.0, 1.0]);
    set_vector(
        &mut values,
        0,
        &parameters.values(&["color", "eyecolor"]),
        3,
    );
    values[3] = bool_float(shader_combo_enabled(shader_key, "BACKGROUND"));
    values
}

fn opacity_values(
    parameters: &MaterialParameters<'_>,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = parameters.scalar(&["alpha", "opacity"], 1.0);
    values[4..8].copy_from_slice(&[1.0, 1.0, 1.0, 1.0]);
    values
}

fn waterwaves_values(
    parameters: &MaterialParameters<'_>,
    shader_key: &str,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    let speed = parameters.scalar(&["speed"], 5.0);
    let scale = parameters.scalar(&["scale"], 200.0);
    values[0] = scene_time_seconds;
    values[1] = speed;
    values[2] = scale;
    values[3] = parameters.scalar(&["strength"], 0.1);
    values[4] = parameters.scalar(&["direction"], 0.0);
    values[5] = parameters.scalar(&["speed2"], speed);
    values[6] = parameters.scalar(&["scale2"], scale);
    values[7] = parameters.scalar(&["direction2"], 0.0);
    values[8] = parameters.scalar(&["offset2"], 0.0);
    values[9] = bool_float(shader_combo_enabled(shader_key, "DUALWAVES"));
    values[10] = parameters.scalar(&["exponent"], 1.0);
    values[12..16].copy_from_slice(&[1.0, 1.0, 1.0, 1.0]);
    values
}

fn waterripple_values(
    parameters: &MaterialParameters<'_>,
    shader_key: &str,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    let ratio = parameters.scalar(&["ratio"], 1.0);
    values[0] = scene_time_seconds;
    values[1] = parameters.scalar(&["animationspeed"], 0.15);
    values[2] = parameters.scalar(&["scale"], 1.0);
    values[3] = parameters.scalar(&["scrollspeed"], 0.0);
    values[4] = parameters.scalar(&["scrolldirection", "direction"], 0.0);
    values[5] = parameters.scalar(&["ripplestrength", "strength"], 0.1);
    values[6] = ratio;
    values[7] = 1.0;
    values[8] = bool_float(shader_texture_slot_enabled(shader_key, 1));
    values[9] = bool_float(shader_texture_slot_enabled(shader_key, 2));
    values[11] = ratio;
    values[12..16].copy_from_slice(&[1.0, 1.0, 1.0, 1.0]);
    values
}

fn set_vector(values: &mut [f32], start: usize, parameter: &[f32], count: usize) {
    for (lane, value) in parameter.iter().take(count).enumerate() {
        if let Some(destination) = values.get_mut(start + lane) {
            *destination = *value;
        }
    }
}

fn shader_combo_enabled(shader_key: &str, name: &str) -> bool {
    let prefix = format!("{}_", name.to_ascii_uppercase());
    shader_key.split("__").any(|part| {
        part.to_ascii_uppercase()
            .strip_prefix(&prefix)
            .and_then(|value| value.parse::<i64>().ok())
            .is_some_and(|value| value != 0)
    })
}

fn shader_texture_slot_enabled(shader_key: &str, slot: u32) -> bool {
    shader_key
        .split("__")
        .find_map(|part| {
            part.strip_prefix("SLOTS_")
                .or_else(|| part.strip_prefix("slots_"))
                .and_then(|mask| u32::from_str_radix(mask, 16).ok())
        })
        .is_some_and(|mask| slot < 32 && mask & (1 << slot) != 0)
}

fn bool_float(value: bool) -> f32 {
    if value { 1.0 } else { 0.0 }
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
            for value in value.split_ascii_whitespace() {
                if let Ok(value) = value.parse::<f32>() {
                    out.push(value);
                }
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

        let payload = pack_scene_material_uniforms(&storage, &[draw], 0.0);

        assert_eq!(payload.len(), SCENE_MATERIAL_UNIFORM_FLOATS * size_of::<f32>());
        assert!(payload.iter().all(|byte| *byte == 0));
    }

    #[test]
    fn material_uniform_packs_color_constant_into_first_vec4() {
        let storage = storage_with_constants(
            "genericimage4",
            &[("tint", "[0.25,0.5,0.75,0.9]")],
        );
        let draw = draw_with_material(SceneMaterialHandle(0));

        let payload = pack_scene_material_uniforms(&storage, &[draw], 0.0);

        assert_eq!(f32_from_payload(&payload, 0), 0.25);
        assert_eq!(f32_from_payload(&payload, 4), 0.5);
        assert_eq!(f32_from_payload(&payload, 8), 0.75);
        assert_eq!(f32_from_payload(&payload, 12), 0.9);
    }

    #[test]
    fn waterwaves_uniform_uses_named_lanes_and_scene_time() {
        let storage = storage_with_constants(
            "effects/waterwaves__SLOTS_3__DUALWAVES_1",
            &[
                ("speed", "2.0"),
                ("scale", "5.0"),
                ("strength", "0.03"),
                ("direction", "-0.9"),
                ("speed2", "3.0"),
            ],
        );
        let payload = pack_scene_material_uniforms(
            &storage,
            &[draw_with_material(SceneMaterialHandle(0))],
            4.25,
        );

        assert_eq!(f32_from_payload(&payload, 0), 4.25);
        assert_eq!(f32_from_payload(&payload, 4), 2.0);
        assert_eq!(f32_from_payload(&payload, 8), 5.0);
        assert_eq!(f32_from_payload(&payload, 12), 0.03);
        assert_eq!(f32_from_payload(&payload, 16), -0.9);
        assert_eq!(f32_from_payload(&payload, 20), 3.0);
        assert_eq!(f32_from_payload(&payload, 36), 1.0);
    }

    #[test]
    fn opacity_uniform_maps_instance_alpha() {
        let storage = storage_with_constants(
            "effects/opacity__SLOTS_1",
            &[("alpha", "0.97")],
        );
        let payload = pack_scene_material_uniforms(
            &storage,
            &[draw_with_material(SceneMaterialHandle(0))],
            0.0,
        );

        assert_eq!(f32_from_payload(&payload, 0), 0.97);
    }

    fn storage_with_constants(shader: &str, constants: &[(&str, &str)]) -> SceneStorage {
        let mut strings = vec![shader.to_owned()];
        let mut material_constants = Vec::with_capacity(constants.len());
        for (name, value) in constants {
            let name_id = SceneStringId(strings.len() as u32);
            strings.push((*name).to_owned());
            let value_id = SceneStringId(strings.len() as u32);
            strings.push((*value).to_owned());
            material_constants.push(SceneMaterialConstantRecord {
                name: name_id,
                value_json: value_id,
            });
        }
        SceneStorage::from_document(SceneBinaryDocument {
            strings,
            materials: vec![SceneMaterialRecord {
                id: SceneMaterialHandle(0),
                resource: SceneResourceId::NONE,
                pass_start: 0,
                pass_count: 1,
            }],
            material_passes: vec![SceneMaterialPassRecord {
                material: SceneMaterialHandle(0),
                shader_key: SceneStringId(0),
                target: SceneStringId::NONE,
                texture_start: 0,
                texture_count: 0,
                constant_start: 0,
                constant_count: material_constants.len() as u32,
                pipeline_blend: ScenePipelineBlend::Normal,
                depth_test: SceneDepthTest::Disabled,
                depth_write: false,
                cull_mode: SceneCullMode::None,
                alpha_writing: SceneStringId::NONE,
                clear_target: false,
            }],
            material_constants,
            ..SceneBinaryDocument::default()
        })
        .expect("storage")
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
