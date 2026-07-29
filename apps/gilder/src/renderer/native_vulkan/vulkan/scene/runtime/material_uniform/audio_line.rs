//! Typed uniform packing for the authored audio-line effect.

use super::{MaterialParameters, SCENE_MATERIAL_UNIFORM_FLOATS, set_vector};
use crate::engine::scene::StereoSpectrum64;

pub(super) fn audio_line_values(
    parameters: &MaterialParameters<'_>,
    spectrum: Option<&StereoSpectrum64>,
) -> [f32; SCENE_MATERIAL_UNIFORM_FLOATS] {
    let mut values = [0.0; SCENE_MATERIAL_UNIFORM_FLOATS];
    values[0..3].copy_from_slice(&[1.0; 3]);
    set_vector(&mut values, 0, &parameters.values(&["曲线颜色"]), 3);
    values[3] = parameters.scalar(&["曲线透明度"], 1.0);
    values[4] = parameters.scalar(&["整体振幅"], 0.4);
    values[5] = parameters.scalar(&["频率范围 (0-63)"], 48.0);
    values[6] = parameters.scalar(&["包络线陡峭度"], 2.0);
    values[7] = parameters.scalar(&["曲线粗细"], 0.003);
    values[8] = parameters.scalar(&["曲线平滑度 (抗锯齿)"], 0.003);
    values[9] = parameters.scalar(&["垂直位置 (基线)"], 0.0);
    if let Some(spectrum) = spectrum {
        values[16..80].copy_from_slice(&spectrum.left);
        values[80..144].copy_from_slice(&spectrum.right);
    }
    values
}
