//! Typed uniform packing for authored workshop effect families.

use super::{
    MaterialParameters, SCENE_MATERIAL_UNIFORM_FLOATS, material_texture_resolution, set_vector,
};
use crate::engine::scene::{SceneRenderingDeviceMeshDraw, SceneStorage};

pub(super) fn clipping_mask_values(
    parameters: &MaterialParameters<'_>,
    storage: &SceneStorage,
    draw: &SceneRenderingDeviceMeshDraw,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = parameters.scalar(&["0opacity"], 1.0);
    values[1] = parameters.scalar(&["weight"], 1.0);
    values[2] = parameters.scalar(&["threshold"], 1.0);
    values[3] = parameters.scalar(&["1texOffset"], 0.0);
    values[4..7].copy_from_slice(&[1.0; 3]);
    set_vector(&mut values, 4, &parameters.values(&["color"]), 3);
    let [width, height] = draw.authored_source_extent;
    values[8..12].copy_from_slice(&[
        width.max(1.0),
        height.max(1.0),
        width.max(1.0),
        height.max(1.0),
    ]);
    let mut clip_resolution = material_texture_resolution(storage, parameters.pass, 1);
    if clip_resolution == [1.0; 4] {
        // First-class effect targets are graph resources rather than static
        // SceneTexture records. Their authored clipping-mask contract uses the
        // same local extent as the consuming effect graph.
        clip_resolution = [
            width.max(1.0),
            height.max(1.0),
            width.max(1.0),
            height.max(1.0),
        ];
    }
    values[12..16].copy_from_slice(&clip_resolution);
    values
}

pub(super) fn custom_user_texture_values(
    parameters: &MaterialParameters<'_>,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = parameters.scalar(&["multiply"], 1.0);
    values
}

pub(super) fn gradient_color_values(
    parameters: &MaterialParameters<'_>,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = scene_time_seconds;
    values[1] = parameters.scalar(&["Amount"], 1.5);
    values[2] = parameters.scalar(&["Hue Speed"], 0.0);
    values[3] = parameters.scalar(&["Oscillate"], 0.0);
    values[4..7].copy_from_slice(&[1.0, 0.0, 0.2]);
    set_vector(&mut values, 4, &parameters.values(&["Color 1"]), 3);
    values[7] = parameters.scalar(&["Opacity"], 1.0);
    values[8..11].copy_from_slice(&[0.0, 0.0, 1.0]);
    set_vector(&mut values, 8, &parameters.values(&["Color 2"]), 3);
    values
}

pub(super) fn ring_values(
    parameters: &MaterialParameters<'_>,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0..3].copy_from_slice(&[1.0, 0.0, 0.0]);
    set_vector(&mut values, 0, &parameters.values(&["圆环颜色"]), 3);
    values[3] = parameters.scalar(&["圆环大小"], 0.3);
    values[4..7].copy_from_slice(&[0.0, 0.0, 1.0]);
    set_vector(&mut values, 4, &parameters.values(&["圆环颜色2"]), 3);
    values[7] = parameters.scalar(&["圆环宽度"], 0.03);
    values[8] = parameters.scalar(&["渐变旋转"], 0.0);
    values[9] = parameters.scalar(&["边缘模糊"], 0.01);
    values[10] = parameters.scalar(&["缺口大小"], 280.0);
    values[12] = parameters.scalar(&["圆角半径"], 1.0);
    values[13] = parameters.scalar(&["圆环透明度"], 1.0);
    values
}

pub(super) fn sphere_values(
    parameters: &MaterialParameters<'_>,
    scene_time_seconds: f32,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0] = scene_time_seconds;
    values[1] = parameters.scalar(&["液体滚动速度"], 0.5);
    values[2] = parameters.scalar(&["球体大小"], 2.0);
    values[3] = parameters.scalar(&["球效果透明度"], 1.0);
    values[4..7].copy_from_slice(&[1.0, 0.2, 0.8]);
    set_vector(&mut values, 4, &parameters.values(&["颜色1"]), 3);
    values[8..11].copy_from_slice(&[0.2, 0.8, 1.0]);
    set_vector(&mut values, 8, &parameters.values(&["颜色2"]), 3);
    values[12..15].copy_from_slice(&[0.8, 0.4, 1.0]);
    set_vector(&mut values, 12, &parameters.values(&["球体颜色"]), 3);
    values[15] = parameters.scalar(&["球体纯色透明度"], 1.0);
    values
}
