//! Per-draw vertex-stage uniform packing for mesh and fullscreen effect shaders.
//!
//! References:
//! - `docs/gilder/gilder-scene-engine-architecture.md`
//! - `reverse-engineered/gilder/effects/iris.md`
//! - `reverse-engineered/gilder/shaders/effects/iris.vert`
//! - `reverse-engineered/gilder/docs/exe/global-uniforms.md`

use std::mem::size_of;

use crate::engine::scene::{
    INVALID_MATERIAL_ID, INVALID_OBJECT_ID, SceneMaterialHandle, SceneRenderingDeviceMeshDraw,
    SceneStorage,
};
use crate::renderer::native_vulkan::scene::{
    BuiltinSceneParameterLayout, native_vulkan_scene_shader_for_key,
};

use super::flat_rounded_mask_coverage::{
    FlatRoundedMaskUvBounds, flat_rounded_effect_enabled, flat_rounded_mask_uv_bounds,
};
use super::material_uniform::{draw_parameter_layout, material_parameter_values};
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
    pack_scene_draw_uniforms_into(
        &mut payload,
        storage,
        draws,
        scene_time_seconds,
        output_extent,
    );
    payload
}

pub(super) fn pack_scene_draw_uniforms_into(
    payload: &mut Vec<u8>,
    storage: &SceneStorage,
    draws: &[SceneRenderingDeviceMeshDraw],
    scene_time_seconds: f32,
    output_extent: [u32; 2],
) {
    payload.clear();
    payload.reserve(
        draws
            .len()
            .saturating_mul(SCENE_DRAW_UNIFORM_BYTES as usize)
            .saturating_sub(payload.capacity()),
    );
    for draw in draws {
        let layout = draw_parameter_layout(storage, draw);
        let actual_shader = storage.string(draw.shader_key).unwrap_or_default();
        let mut values = if actual_shader.eq_ignore_ascii_case("we/objectcomposite")
            || actual_shader.eq_ignore_ascii_case("we/framebuffer-water-quantized-shake-final")
        {
            projected_object_uv_draw_values(storage, draw, output_extent)
        } else {
            match layout {
                BuiltinSceneParameterLayout::Iris => {
                    iris_draw_values(storage, draw.material, scene_time_seconds)
                }
                BuiltinSceneParameterLayout::WaterWaves
                | BuiltinSceneParameterLayout::WaterWavesUvField => {
                    waterwaves_draw_values(storage, draw, output_extent)
                }
                BuiltinSceneParameterLayout::AudioBars
                | BuiltinSceneParameterLayout::TechCircle => {
                    effect_uv_affine_draw_values(storage, draw, output_extent)
                }
                BuiltinSceneParameterLayout::Oscilloscope
                    if draw.primitive
                        == crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh =>
                {
                    matrix_draw_values(scene_cover_clip_transform(
                        storage.project(),
                        output_extent,
                        draw.clip_transform,
                    ))
                }
                BuiltinSceneParameterLayout::Blend
                | BuiltinSceneParameterLayout::BlendGradient
                | BuiltinSceneParameterLayout::Oscilloscope
                | BuiltinSceneParameterLayout::Scroll
                | BuiltinSceneParameterLayout::Skew => {
                    effect_uv_affine_draw_values(storage, draw, output_extent)
                }
                BuiltinSceneParameterLayout::Caustics
                    if material_shader_key(storage, draw.material).is_some_and(|key| {
                        key.to_ascii_lowercase()
                            .contains("__gilder_framebuffer_quantized_overlay_1")
                    }) =>
                {
                    projected_object_uv_draw_values(storage, draw, output_extent)
                }
                BuiltinSceneParameterLayout::FinalEffectProgram
                    if material_shader_key(storage, draw.material).is_some_and(|key| {
                        key.eq_ignore_ascii_case("we/flat-rounded-opacity-final")
                    }) =>
                {
                    rounded_mask_support_quad_draw_values(storage, draw, output_extent)
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
                    effect_uv_affine_draw_values(storage, draw, output_extent)
                }
                _ => matrix_draw_values(scene_cover_clip_transform(
                    storage.project(),
                    output_extent,
                    draw.clip_transform,
                )),
            }
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
    debug_assert_eq!(
        payload.len(),
        draws.len() * SCENE_DRAW_UNIFORM_BYTES as usize
    );
}

fn material_shader_key(storage: &SceneStorage, material: SceneMaterialHandle) -> Option<&str> {
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
    effect_uv_affine_draw_values(storage, draw, output_extent)
}

fn effect_uv_affine_draw_values(
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
    output_extent: [u32; 2],
) -> [f32; SCENE_DRAW_UNIFORM_FLOATS] {
    if draw.primitive
        == crate::engine::scene::SceneRenderingDeviceDrawPrimitive::ObjectMesh
    {
        return projected_object_uv_draw_values(storage, draw, output_extent);
    }
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
    let sampled_source_extent = storage
        .string(draw.shader_key)
        .is_some_and(|shader| shader == "we/flat-rounded-hsl-source")
        .then_some(draw.authored_source_extent);
    let uv_bounds = if flat_rounded_effect_enabled(draw) {
        flat_rounded_mask_uv_bounds(
            size,
            softness,
            object_pixel_extent,
            output_extent,
            sampled_source_extent,
        )
    } else {
        Some(FlatRoundedMaskUvBounds {
            min: [0.0, 0.0],
            max: [1.0, 1.0],
        })
    }
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
#[path = "draw_uniform/tests.rs"]
mod tests;
