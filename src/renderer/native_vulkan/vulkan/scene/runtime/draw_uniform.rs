//! Per-draw vertex-stage uniform packing for mesh and fullscreen effect shaders.
//!
//! References:
//! - `docs/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/effects/iris.md`
//! - `reverse-engineered/shaders/effects/iris.vert`
//! - `reverse-engineered/docs/exe/global-uniforms.md`

use std::mem::size_of;

use crate::engine::scene::{
    INVALID_MATERIAL_ID, INVALID_OBJECT_ID, SceneMaterialHandle, SceneRenderingDeviceMeshDraw,
    SceneStorage,
};
use crate::renderer::native_vulkan::scene::{
    BuiltinSceneParameterLayout, native_vulkan_scene_shader_for_key,
};

use super::flat_rounded_mask_coverage::{FlatRoundedMaskUvBounds, flat_rounded_mask_uv_bounds};
use super::material_uniform::{material_parameter_layout, material_parameter_values};
use super::scene_viewport::scene_cover_clip_transform;

pub(super) const SCENE_DRAW_UNIFORM_BYTES: u64 = 64;
const SCENE_DRAW_UNIFORM_FLOATS: usize = SCENE_DRAW_UNIFORM_BYTES as usize / size_of::<f32>();

pub(super) fn pack_scene_draw_uniforms(
    storage: &SceneStorage,
    draws: &[SceneRenderingDeviceMeshDraw],
    scene_time_seconds: f32,
    output_extent: [u32; 2],
) -> Vec<u8> {
    let mut payload = Vec::with_capacity(draws.len() * SCENE_DRAW_UNIFORM_BYTES as usize);
    for draw in draws {
        let layout = material_parameter_layout(storage, draw.material);
        let mut values = match layout {
            BuiltinSceneParameterLayout::Iris => {
                iris_draw_values(storage, draw.material, scene_time_seconds)
            }
            BuiltinSceneParameterLayout::WaterWaves
            | BuiltinSceneParameterLayout::WaterWavesUvField => {
                waterwaves_draw_values(storage, draw, output_extent)
            }
            BuiltinSceneParameterLayout::AudioBars | BuiltinSceneParameterLayout::TechCircle => {
                identity_uv_affine_rows()
            }
            BuiltinSceneParameterLayout::Blend
            | BuiltinSceneParameterLayout::BlendGradient
            | BuiltinSceneParameterLayout::Oscilloscope
            | BuiltinSceneParameterLayout::Scroll
            | BuiltinSceneParameterLayout::Skew => {
                object_local_effect_draw_values(storage, draw, output_extent)
            }
            BuiltinSceneParameterLayout::Caustics
                if material_shader_key(storage, draw.material).is_some_and(|key| {
                    key.to_ascii_lowercase()
                        .contains("__gilder_framebuffer_overlay_1")
                }) =>
            {
                projected_object_uv_draw_values(storage, draw, output_extent)
            }
            BuiltinSceneParameterLayout::FinalEffectProgram
                if material_shader_key(storage, draw.material)
                    .is_some_and(|key| key.eq_ignore_ascii_case("we/flat-rounded-opacity-final")) =>
            {
                rounded_mask_support_quad_draw_values(storage, draw, output_extent)
            }
            BuiltinSceneParameterLayout::FinalEffectProgram
                if material_shader_key(storage, draw.material)
                    .is_some_and(|key| {
                        key.eq_ignore_ascii_case("we/framebuffer-water-final")
                            || key.eq_ignore_ascii_case("we/framebuffer-water-post-final")
                    }) =>
            {
                projected_object_uv_draw_values(storage, draw, output_extent)
            }
            BuiltinSceneParameterLayout::RoundedMask => {
                if draw.primitive
                    == crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectUvSupportQuad
                {
                    rounded_mask_support_quad_draw_values(storage, draw, output_extent)
                } else {
                    rounded_mask_draw_values(storage, draw, output_extent)
                }
            }
            BuiltinSceneParameterLayout::WaterFlow => {
                object_local_effect_draw_values(storage, draw, output_extent)
            }
            _ => matrix_draw_values(scene_cover_clip_transform(
                storage.project(),
                output_extent,
                draw.clip_transform,
            )),
        };
        if layout == BuiltinSceneParameterLayout::WaterWavesUvField {
            values[3] = if draw.effect_batch_atlas_tile == INVALID_OBJECT_ID {
                0.0
            } else {
                draw.effect_batch_atlas_tile as f32
            };
            values[7] = draw.effect_batch_atlas_grid[0].max(1) as f32;
            values[11] = draw.effect_batch_atlas_grid[1].max(1) as f32;
        }
        for value in values {
            payload.extend_from_slice(&value.to_le_bytes());
        }
    }
    payload
}

fn material_shader_key(
    storage: &SceneStorage,
    material: SceneMaterialHandle,
) -> Option<&str> {
    storage
        .material(material)
        .and_then(|material| storage.material_passes(material).first())
        .and_then(|pass| storage.string(pass.shader_key))
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

fn waterwaves_draw_values(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    output_extent: [u32; 2],
) -> [f32; SCENE_DRAW_UNIFORM_FLOATS] {
    object_local_effect_draw_values(storage, draw, output_extent)
}

fn object_local_effect_draw_values(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    output_extent: [u32; 2],
) -> [f32; SCENE_DRAW_UNIFORM_FLOATS] {
    if object_effects_use_authored_texture_space(storage, draw) {
        return identity_uv_affine_rows();
    }
    projected_object_uv_draw_values(storage, draw, output_extent)
}

fn projected_object_uv_draw_values(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    output_extent: [u32; 2],
) -> [f32; SCENE_DRAW_UNIFORM_FLOATS] {
    projected_object_uv_affine(storage, draw, output_extent).unwrap_or_else(identity_uv_affine_rows)
}

fn projected_object_uv_affine(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    output_extent: [u32; 2],
) -> Option<[f32; SCENE_DRAW_UNIFORM_FLOATS]> {
    let clip_transform =
        scene_cover_clip_transform(storage.project(), output_extent, draw.clip_transform);
    object_uv_affine_rows(clip_transform, draw.authored_source_extent)
}

fn rounded_mask_draw_values(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    output_extent: [u32; 2],
) -> [f32; SCENE_DRAW_UNIFORM_FLOATS] {
    let clip_transform =
        scene_cover_clip_transform(storage.project(), output_extent, draw.clip_transform);
    let Some(projected_affine) = object_uv_affine_rows(clip_transform, draw.authored_source_extent)
    else {
        return identity_uv_affine_rows();
    };
    let mut values = if object_effects_use_authored_texture_space(storage, draw) {
        identity_uv_affine_rows()
    } else {
        projected_affine
    };
    if let Some([width_pixels, height_pixels]) =
        projected_object_pixel_extent(projected_affine, output_extent)
    {
        values[8] = width_pixels;
        values[9] = height_pixels;
        values[10] = output_extent[0].max(output_extent[1]).max(1) as f32;
    }
    values
}

fn rounded_mask_support_quad_draw_values(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    output_extent: [u32; 2],
) -> [f32; SCENE_DRAW_UNIFORM_FLOATS] {
    let Some(projected_affine) = projected_object_uv_affine(storage, draw, output_extent) else {
        return identity_rounded_mask_support_quad_values(output_extent);
    };
    let object_pixel_extent = projected_object_pixel_extent(projected_affine, output_extent)
        .unwrap_or([output_extent[0] as f32, output_extent[1] as f32]);
    let size_values = material_parameter_values(storage, draw.material, &["Size"]);
    let size = [
        size_values.first().copied().unwrap_or(1.0),
        size_values.get(1).copied().unwrap_or(1.0),
    ];
    let softness = material_parameter_values(storage, draw.material, &["Softness"])
        .first()
        .copied()
        .unwrap_or(0.5);
    let uv_bounds = flat_rounded_mask_uv_bounds(size, softness, object_pixel_extent, output_extent)
        .unwrap_or_else(|| full_target_object_uv_bounds(projected_affine));
    let mut values = [0.0; SCENE_DRAW_UNIFORM_FLOATS];
    values[0..3].copy_from_slice(&projected_affine[8..11]);
    values[4..7].copy_from_slice(&projected_affine[12..15]);
    values[8] = object_pixel_extent[0];
    values[9] = object_pixel_extent[1];
    values[10] = output_extent[0].max(output_extent[1]).max(1) as f32;
    values[12..16].copy_from_slice(&[
        uv_bounds.min[0],
        uv_bounds.min[1],
        uv_bounds.max[0],
        uv_bounds.max[1],
    ]);
    values
}

fn identity_rounded_mask_support_quad_values(
    output_extent: [u32; 2],
) -> [f32; SCENE_DRAW_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_DRAW_UNIFORM_FLOATS];
    values[0] = 1.0;
    values[5] = 1.0;
    values[8] = output_extent[0].max(1) as f32;
    values[9] = output_extent[1].max(1) as f32;
    values[10] = output_extent[0].max(output_extent[1]).max(1) as f32;
    values[14] = 1.0;
    values[15] = 1.0;
    values
}

fn full_target_object_uv_bounds(
    affine: [f32; SCENE_DRAW_UNIFORM_FLOATS],
) -> FlatRoundedMaskUvBounds {
    let mut min = [f32::INFINITY; 2];
    let mut max = [f32::NEG_INFINITY; 2];
    for screen_uv in [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]] {
        let object_uv = [
            affine[0] * screen_uv[0] + affine[1] * screen_uv[1] + affine[2],
            affine[4] * screen_uv[0] + affine[5] * screen_uv[1] + affine[6],
        ];
        for lane in 0..2 {
            min[lane] = min[lane].min(object_uv[lane]);
            max[lane] = max[lane].max(object_uv[lane]);
        }
    }
    FlatRoundedMaskUvBounds { min, max }
}

fn projected_object_pixel_extent(
    affine: [f32; SCENE_DRAW_UNIFORM_FLOATS],
    output_extent: [u32; 2],
) -> Option<[f32; 2]> {
    let output_width = output_extent[0].max(1) as f32;
    let output_height = output_extent[1].max(1) as f32;
    let object_x_gradient = (affine[0] / output_width).hypot(affine[1] / output_height);
    let object_y_gradient = (affine[4] / output_width).hypot(affine[5] / output_height);
    let extents = [object_x_gradient.recip(), object_y_gradient.recip()];
    extents
        .iter()
        .all(|value| value.is_finite() && *value > 0.0)
        .then_some(extents)
}

fn object_effects_use_authored_texture_space(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
) -> bool {
    let Some(object) = storage.objects().get(draw.object.0 as usize) else {
        return false;
    };
    let Some(graph) = storage.render_graphs().get(object.render_graph as usize) else {
        return false;
    };
    storage.render_graph_passes(graph).iter().any(|pass| {
        storage.string(pass.shader_key).is_some_and(|shader| {
                    shader.eq_ignore_ascii_case("we/image-effect-source")
                        || shader.eq_ignore_ascii_case("we/puppet-effect-source")
                        || shader.eq_ignore_ascii_case("we/waterwaves-uv-field")
        })
    })
}

pub(super) fn object_uv_to_screen_linear(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    output_extent: [u32; 2],
) -> Option<[[f32; 2]; 2]> {
    let affine = object_uv_to_screen_affine(storage, draw, output_extent)?;
    Some([[affine[0][0], affine[0][1]], [affine[1][0], affine[1][1]]])
}

pub(super) fn object_uv_to_screen_affine(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    output_extent: [u32; 2],
) -> Option<[[f32; 3]; 2]> {
    let affine = projected_object_uv_affine(storage, draw, output_extent)?;
    Some([
        [affine[8], affine[9], affine[10]],
        [affine[12], affine[13], affine[14]],
    ])
}

pub(super) fn object_projected_pixel_extent(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    output_extent: [u32; 2],
) -> Option<[f32; 2]> {
    projected_object_pixel_extent(
        projected_object_uv_affine(storage, draw, output_extent)?,
        output_extent,
    )
}

fn object_uv_affine_rows(
    clip_transform: [[f32; 4]; 4],
    source_extent: [f32; 2],
) -> Option<[f32; SCENE_DRAW_UNIFORM_FLOATS]> {
    let [width, height] = source_extent;
    let homogeneous_w = clip_transform[3][3];
    if !width.is_finite()
        || !height.is_finite()
        || width <= 0.0
        || height <= 0.0
        || !homogeneous_w.is_finite()
        || homogeneous_w.abs() <= 1.0e-8
        || clip_transform[3][0].abs() > 1.0e-8
        || clip_transform[3][1].abs() > 1.0e-8
    {
        return None;
    }

    let clip_xx = clip_transform[0][0] / homogeneous_w;
    let clip_xy = clip_transform[0][1] / homogeneous_w;
    let clip_xt = clip_transform[0][3] / homogeneous_w;
    let clip_yx = clip_transform[1][0] / homogeneous_w;
    let clip_yy = clip_transform[1][1] / homogeneous_w;
    let clip_yt = clip_transform[1][3] / homogeneous_w;
    let object_to_screen_xx = 0.5 * clip_xx * width;
    let object_to_screen_xy = -0.5 * clip_xy * height;
    let object_to_screen_xt =
        0.5 * (1.0 + clip_xt - 0.5 * clip_xx * width + 0.5 * clip_xy * height);
    let object_to_screen_yx = 0.5 * clip_yx * width;
    let object_to_screen_yy = -0.5 * clip_yy * height;
    let object_to_screen_yt =
        0.5 * (1.0 + clip_yt - 0.5 * clip_yx * width + 0.5 * clip_yy * height);
    let determinant =
        object_to_screen_xx * object_to_screen_yy - object_to_screen_xy * object_to_screen_yx;
    if !determinant.is_finite() || determinant.abs() <= 1.0e-8 {
        return None;
    }

    let inverse_determinant = determinant.recip();
    let screen_to_object_xx = object_to_screen_yy * inverse_determinant;
    let screen_to_object_xy = -object_to_screen_xy * inverse_determinant;
    let screen_to_object_yx = -object_to_screen_yx * inverse_determinant;
    let screen_to_object_yy = object_to_screen_xx * inverse_determinant;
    let screen_to_object_xt =
        -(screen_to_object_xx * object_to_screen_xt + screen_to_object_xy * object_to_screen_yt);
    let screen_to_object_yt =
        -(screen_to_object_yx * object_to_screen_xt + screen_to_object_yy * object_to_screen_yt);
    let values = [
        screen_to_object_xx,
        screen_to_object_xy,
        screen_to_object_xt,
        0.0,
        screen_to_object_yx,
        screen_to_object_yy,
        screen_to_object_yt,
        0.0,
        object_to_screen_xx,
        object_to_screen_xy,
        object_to_screen_xt,
        0.0,
        object_to_screen_yx,
        object_to_screen_yy,
        object_to_screen_yt,
        0.0,
    ];
    values
        .iter()
        .all(|value| value.is_finite())
        .then_some(values)
}

fn identity_uv_affine_rows() -> [f32; SCENE_DRAW_UNIFORM_FLOATS] {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0,
    ]
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
        .and_then(|material| {
            storage
                .document()
                .material_passes
                .get(material.pass_start as usize)
        })
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

        let payload = pack_scene_draw_uniforms(&storage, &[draw], 9.0, [1, 1]);

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
            [1, 1],
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

    #[test]
    fn audio_image_local_pass_uses_target_local_uvs() {
        let storage = audio_bars_storage();
        let mut draw = draw_with_material(SceneMaterialHandle(0));
        draw.primitive = SceneRenderingDeviceDrawPrimitive::FullscreenTriangle;
        draw.authored_source_extent = [1000.0, 1000.0];
        draw.clip_transform = [
            [0.000313, 0.0, 0.0, 0.5315],
            [0.0, -0.000557, 0.0, 0.6945],
            [0.0, 0.0, 0.6, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];

        let payload = pack_scene_draw_uniforms(&storage, &[draw], 0.0, [3840, 2160]);

        for (lane, expected) in [
            (0, 1.0),
            (1, 0.0),
            (2, 0.0),
            (4, 0.0),
            (5, 1.0),
            (6, 0.0),
        ] {
            assert_close(payload_f32(&payload, lane * size_of::<f32>()), expected);
        }
    }

    #[test]
    fn waterwaves_keeps_phase_in_object_uv_when_only_translation_differs() {
        let storage = waterwaves_storage();
        let mut shadow = draw_with_material(SceneMaterialHandle(0));
        shadow.primitive = SceneRenderingDeviceDrawPrimitive::FullscreenTriangle;
        shadow.authored_source_extent = [1571.0, 2621.0];
        shadow.clip_transform = [
            [0.0005609375, 0.0, 0.0, 0.022509336],
            [0.0, -0.0009972223, 0.0, -0.033291817],
            [0.0, 0.0, 1.077, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let mut body = shadow;
        body.clip_transform[0][3] = 0.03257537;
        body.clip_transform[1][3] = -0.04262042;

        let payload = pack_scene_draw_uniforms(&storage, &[shadow, body], 0.0, [1, 1]);
        let authored_uv = [0.83, 0.42];
        let shadow_screen_uv = packed_affine_point(&payload, 0, 8, 12, authored_uv);
        let body_screen_uv = packed_affine_point(&payload, 1, 8, 12, authored_uv);
        let shadow_recovered_uv = packed_affine_point(&payload, 0, 0, 4, shadow_screen_uv);
        let body_recovered_uv = packed_affine_point(&payload, 1, 0, 4, body_screen_uv);

        assert!((shadow_screen_uv[0] - body_screen_uv[0]).abs() > 0.001);
        assert!((shadow_screen_uv[1] - body_screen_uv[1]).abs() > 0.001);
        assert_vec2_close(shadow_recovered_uv, authored_uv);
        assert_vec2_close(body_recovered_uv, authored_uv);
        let direction = [0.6_f32, 0.8_f32];
        let shadow_phase_position =
            shadow_recovered_uv[0] * direction[0] + shadow_recovered_uv[1] * direction[1];
        let body_phase_position =
            body_recovered_uv[0] * direction[0] + body_recovered_uv[1] * direction[1];
        assert_close(shadow_phase_position, body_phase_position);
        assert_close(
            payload_f32(&payload, 32),
            0.5 * shadow.clip_transform[0][0] * shadow.authored_source_extent[0],
        );
        assert_close(payload_f32(&payload, 32), payload_f32(&payload, 96));
        assert_close(payload_f32(&payload, 52), payload_f32(&payload, 116));
    }

    #[test]
    fn projected_pixel_extent_uses_screen_to_object_gradient() {
        let affine = [
            0.5, 0.0, 0.0, 0.0, 0.0, 0.25, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, 4.0, 0.0, 0.0,
        ];
        assert_eq!(
            projected_object_pixel_extent(affine, [100, 80]),
            Some([200.0, 320.0])
        );
    }

    #[test]
    fn rounded_mask_support_quad_packs_projected_bounds_and_six_vertex_extent() {
        let storage = rounded_mask_storage();
        let mut draw = draw_with_material(SceneMaterialHandle(0));
        draw.primitive = SceneRenderingDeviceDrawPrimitive::ObjectUvSupportQuad;
        draw.authored_source_extent = [20.0, 40.0];
        draw.clip_transform = [
            [0.1, 0.0, 0.0, 0.0],
            [0.0, -0.05, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];

        let payload = pack_scene_draw_uniforms(&storage, &[draw], 0.0, [100, 100]);

        for (lane, expected) in [
            (0, 1.0),
            (1, 0.0),
            (2, 0.0),
            (4, 0.0),
            (5, 1.0),
            (6, 0.0),
            (8, 100.0),
            (9, 100.0),
            (10, 100.0),
            (12, 0.1),
            (13, 0.2),
            (14, 0.9),
            (15, 0.8),
        ] {
            assert_close(payload_f32(&payload, lane * size_of::<f32>()), expected);
        }
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

    fn waterwaves_storage() -> SceneStorage {
        SceneStorage::from_document(SceneBinaryDocument {
            strings: vec!["effects/waterwaves__SLOTS_3".to_owned()],
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
                constant_count: 0,
                pipeline_blend: ScenePipelineBlend::Normal,
                depth_test: SceneDepthTest::Disabled,
                depth_write: false,
                cull_mode: SceneCullMode::None,
                alpha_writing: SceneStringId::NONE,
                clear_target: false,
            }],
            ..SceneBinaryDocument::default()
        })
        .expect("waterwaves storage")
    }

    fn audio_bars_storage() -> SceneStorage {
        SceneStorage::from_document(SceneBinaryDocument {
            strings: vec![
                "workshop/3082978660/effects/Simple_Audio_Bars__SLOTS_1__SHAPE_7".to_owned(),
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
                constant_count: 0,
                pipeline_blend: ScenePipelineBlend::Normal,
                depth_test: SceneDepthTest::Disabled,
                depth_write: false,
                cull_mode: SceneCullMode::None,
                alpha_writing: SceneStringId::NONE,
                clear_target: false,
            }],
            ..SceneBinaryDocument::default()
        })
        .expect("audio bars storage")
    }

    fn rounded_mask_storage() -> SceneStorage {
        SceneStorage::from_document(SceneBinaryDocument {
            strings: vec![
                "workshop/3083593512/effects/rounded_mask__SLOTS_1__B_SQUARE_0__C_ALPHA_ONLY_0__SOFT_1"
                    .to_owned(),
                "Size".to_owned(),
                "\"0.8 0.6\"".to_owned(),
                "Softness".to_owned(),
                "0".to_owned(),
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
                    name: SceneStringId(1),
                    value_json: SceneStringId(2),
                },
                SceneMaterialConstantRecord {
                    name: SceneStringId(3),
                    value_json: SceneStringId(4),
                },
            ],
            ..SceneBinaryDocument::default()
        })
        .expect("rounded-mask storage")
    }

    fn draw_with_material(material: SceneMaterialHandle) -> SceneRenderingDeviceMeshDraw {
        SceneRenderingDeviceMeshDraw {
            primitive: SceneRenderingDeviceDrawPrimitive::ObjectMesh,
            mesh_index: 0,
            resolved_object_index: 0,
            clip_transform: [[0.0; 4]; 4],
            authored_source_extent: [0.0; 2],
            skinning_palette_start: crate::engine::scene::INVALID_OBJECT_ID,
            skinning_palette_count: 0,
            resolved_color: crate::engine::scene::SceneVec3 {
                x: 1.0,
                y: 1.0,
                z: 1.0,
            },
            resolved_alpha: 1.0,
            apply_resolved_visual: true,
            effect_batch_atlas_tile: u32::MAX,
            effect_batch_atlas_grid: [0; 2],
            object: crate::engine::scene::SceneObjectHandle(0),
            material,
            vertex_start: 0,
            vertex_count: 4,
            index_start: 0,
            index_count: 6,
            instance_count: 1,
        }
    }

    fn payload_f32(payload: &[u8], offset: usize) -> f32 {
        f32::from_le_bytes(payload[offset..offset + 4].try_into().unwrap())
    }

    fn packed_affine_point(
        payload: &[u8],
        draw_index: usize,
        row0: usize,
        row1: usize,
        point: [f32; 2],
    ) -> [f32; 2] {
        let base = draw_index * SCENE_DRAW_UNIFORM_BYTES as usize;
        let apply_row = |row: usize| {
            payload_f32(payload, base + row * size_of::<f32>()) * point[0]
                + payload_f32(payload, base + (row + 1) * size_of::<f32>()) * point[1]
                + payload_f32(payload, base + (row + 2) * size_of::<f32>())
        };
        [apply_row(row0), apply_row(row1)]
    }

    fn assert_vec2_close(actual: [f32; 2], expected: [f32; 2]) {
        assert_close(actual[0], expected[0]);
        assert_close(actual[1], expected[1]);
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1.0e-5,
            "expected {expected}, got {actual}"
        );
    }
}
