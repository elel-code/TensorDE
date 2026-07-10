//! Per-draw vertex-stage uniform packing for mesh and fullscreen effect shaders.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/effects/iris.md`
//! - `reverse-engineered/shaders/effects/iris.vert`
//! - `reverse-engineered/docs/exe/global-uniforms.md`

use std::mem::size_of;

use crate::engine::scene::{
    INVALID_MATERIAL_ID, SceneMaterialHandle, SceneRenderingDeviceMeshDraw, SceneStorage,
};
use crate::renderer::native_vulkan::scene::{
    BuiltinSceneParameterLayout, native_vulkan_scene_shader_for_key,
};

use super::material_uniform::{material_parameter_layout, material_parameter_values};

pub(super) const SCENE_DRAW_UNIFORM_BYTES: u64 = 64;
const SCENE_DRAW_UNIFORM_FLOATS: usize = SCENE_DRAW_UNIFORM_BYTES as usize / size_of::<f32>();

pub(super) fn pack_scene_draw_uniforms(
    storage: &SceneStorage,
    draws: &[SceneRenderingDeviceMeshDraw],
    scene_time_seconds: f32,
) -> Vec<u8> {
    let mut payload =
        Vec::with_capacity(draws.len() * SCENE_DRAW_UNIFORM_BYTES as usize);
    for draw in draws {
        let values = match material_parameter_layout(storage, draw.material) {
            BuiltinSceneParameterLayout::Iris => {
                iris_draw_values(storage, draw.material, scene_time_seconds)
            }
            _ => matrix_draw_values(draw.clip_transform),
        };
        for value in values {
            payload.extend_from_slice(&value.to_le_bytes());
        }
    }
    payload
}

fn matrix_draw_values(matrix: [[f32; 4]; 4]) -> [f32; SCENE_DRAW_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_DRAW_UNIFORM_FLOATS];
    for (destination, value) in values.iter_mut().zip(matrix.into_iter().flatten()) {
        *destination = value;
    }
    values
}

fn iris_draw_values(
    storage: &SceneStorage,
    material: SceneMaterialHandle,
    scene_time_seconds: f32,
) -> [f32; SCENE_DRAW_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_DRAW_UNIFORM_FLOATS];
    let scale = material_parameter_values(storage, material, &["scale"]);
    values[0] = scene_time_seconds;
    values[1] = material_scalar(storage, material, &["speed"], 1.0);
    values[2] = material_scalar(storage, material, &["rough"], 0.2);
    values[3] = material_scalar(storage, material, &["noiseamount"], 0.5);
    values[4] = scale.first().copied().unwrap_or(1.0);
    values[5] = scale.get(1).copied().unwrap_or(values[4]);
    values[6] = material_scalar(storage, material, &["phase"], 0.0);
    values[7] = bool_float(material_shader_combo_enabled(storage, material, "MASK"));
    values[8..12].copy_from_slice(&[1.0, 1.0, 1.0, 1.0]);
    values
}

fn material_scalar(
    storage: &SceneStorage,
    material: SceneMaterialHandle,
    names: &[&str],
    default: f32,
) -> f32 {
    material_parameter_values(storage, material, names)
        .first()
        .copied()
        .unwrap_or(default)
}

fn material_shader_combo_enabled(
    storage: &SceneStorage,
    material: SceneMaterialHandle,
    combo: &str,
) -> bool {
    if material.0 == INVALID_MATERIAL_ID {
        return false;
    }
    let Some(pass) = storage
        .document()
        .materials
        .get(material.0 as usize)
        .and_then(|material| storage.document().material_passes.get(material.pass_start as usize))
    else {
        return false;
    };
    let Some(shader) = storage
        .string(pass.shader_key)
        .and_then(native_vulkan_scene_shader_for_key)
    else {
        return false;
    };
    let prefix = format!("{}_", combo.to_ascii_uppercase());
    shader.key.split("__").any(|part| {
        part.to_ascii_uppercase()
            .strip_prefix(&prefix)
            .and_then(|value| value.parse::<i64>().ok())
            .is_some_and(|value| value != 0)
    })
}

fn bool_float(value: bool) -> f32 {
    if value { 1.0 } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scene::{
        SceneBinaryDocument, SceneCullMode, SceneDepthTest, SceneMaterialConstantRecord,
        SceneMaterialPassRecord, SceneMaterialRecord, ScenePipelineBlend,
        SceneRenderingDeviceDrawPrimitive, SceneResourceId, SceneStringId,
    };

    #[test]
    fn ordinary_draw_uniform_preserves_clip_matrix() {
        let storage = SceneStorage::from_document(SceneBinaryDocument::default()).expect("storage");
        let mut draw = draw_with_material(SceneMaterialHandle(INVALID_MATERIAL_ID));
        draw.clip_transform = [
            [1.0, 2.0, 3.0, 4.0],
            [5.0, 6.0, 7.0, 8.0],
            [9.0, 10.0, 11.0, 12.0],
            [13.0, 14.0, 15.0, 16.0],
        ];

        let payload = pack_scene_draw_uniforms(&storage, &[draw], 9.0);

        assert_eq!(payload.len(), SCENE_DRAW_UNIFORM_BYTES as usize);
        assert_eq!(payload_f32(&payload, 0), 1.0);
        assert_eq!(payload_f32(&payload, 60), 16.0);
    }

    #[test]
    fn iris_draw_uniform_maps_named_constants_and_time() {
        let storage = iris_storage();
        let payload = pack_scene_draw_uniforms(
            &storage,
            &[draw_with_material(SceneMaterialHandle(0))],
            3.25,
        );

        assert_eq!(payload_f32(&payload, 0), 3.25);
        assert_eq!(payload_f32(&payload, 4), 1.5);
        assert_eq!(payload_f32(&payload, 8), 0.35);
        assert_eq!(payload_f32(&payload, 12), 0.75);
        assert_eq!(payload_f32(&payload, 16), 2.0);
        assert_eq!(payload_f32(&payload, 20), 3.0);
        assert_eq!(payload_f32(&payload, 24), 0.4);
        assert_eq!(payload_f32(&payload, 28), 1.0);
    }

    fn iris_storage() -> SceneStorage {
        SceneStorage::from_document(SceneBinaryDocument {
            strings: vec![
                "effects/iris__SLOTS_3__MASK_1".to_owned(),
                "scale".to_owned(),
                "\"2 3\"".to_owned(),
                "speed".to_owned(),
                "1.5".to_owned(),
                "rough".to_owned(),
                "0.35".to_owned(),
                "noiseamount".to_owned(),
                "0.75".to_owned(),
                "phase".to_owned(),
                "0.4".to_owned(),
            ],
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
                constant_count: 5,
                pipeline_blend: ScenePipelineBlend::Normal,
                depth_test: SceneDepthTest::Disabled,
                depth_write: false,
                cull_mode: SceneCullMode::None,
                alpha_writing: SceneStringId::NONE,
                clear_target: false,
            }],
            material_constants: (0..5)
                .map(|index| SceneMaterialConstantRecord {
                    name: SceneStringId(1 + index * 2),
                    value_json: SceneStringId(2 + index * 2),
                })
                .collect(),
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

    fn payload_f32(payload: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap())
    }
}
